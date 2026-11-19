//! The callback relay companion.
//!
//! Every callback interface and foreign trait vtable goes through generated
//! native proxies when the companion is bundled. On the owning isolate's
//! thread a proxy calls the Dart function inline, as `Pointer.fromFunction`
//! would. From any other thread it queues the call to the isolate and wakes
//! it through a port: calls that return nothing don't wait, and calls that
//! return something block until the isolate has run them. If the isolate has
//! exited, calls that return nothing and can't throw are dropped as if they
//! had succeeded, and other calls fail or complete with an unexpected error.
//! The companion also carries the wake-ups of Rust futures that Dart awaits,
//! so a future that outlives its isolate doesn't call into it.
use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{bail, Result};
use camino::Utf8Path;
use uniffi_bindgen::interface::{FfiCallbackFunction, FfiDefinition, FfiStruct, FfiType, Method};
use uniffi_bindgen::ComponentInterface;

use super::{fnv1a64, native_type, result_type, CORE_RUNTIME};
use crate::gen::callback_interface::droppable;
use crate::gen::Config;

/// How a proxy delivers calls made off the owning isolate's thread.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Delivery {
    /// A sync method that returns a value or can throw, or `uniffi_clone`:
    /// Rust needs the result, so the caller blocks.
    Wait,
    /// A sync method that returns nothing and can't throw: queued.
    Void,
    /// An async method that returns a value or can throw: queued, completing
    /// through Rust's callback. If the isolate has gone, it completes with an
    /// unexpected error.
    Async,
    /// An async method that returns nothing and can't throw: queued like
    /// `Async`. If the isolate has gone, it completes successfully, since an
    /// unexpected error would only make Rust panic.
    AsyncVoid,
    /// `uniffi_free`: queued.
    Free,
}

/// Every type with a callback vtable in `ci`: callback interfaces and
/// foreign-implementable traits, with their methods.
pub fn callback_types(ci: &ComponentInterface) -> Vec<(String, Vec<Method>)> {
    ci.callback_interface_definitions()
        .iter()
        .map(|c| (c.name().to_owned(), c.methods().into_iter().cloned().collect()))
        .chain(
            ci.object_definitions()
                .iter()
                .filter(|o| o.has_callback_interface())
                .map(|o| (o.name().to_owned(), o.methods().into_iter().cloned().collect())),
        )
        .collect()
}

pub fn has_callbacks(ci: &ComponentInterface) -> bool {
    !callback_types(ci).is_empty()
}

pub fn delivery(field: &str, methods: &[Method]) -> Result<Delivery> {
    Ok(match field {
        "uniffi_free" => Delivery::Free,
        "uniffi_clone" => Delivery::Wait,
        name => {
            let Some(m) = methods.iter().find(|m| m.name() == name) else {
                bail!("no callback method for vtable field `{name}`")
            };
            match (m.is_async(), droppable(m)) {
                (true, true) => Delivery::AsyncVoid,
                (true, false) => Delivery::Async,
                (false, true) => Delivery::Void,
                (false, false) => Delivery::Wait,
            }
        }
    })
}

/// Crate and library basename of the companion.
pub fn crate_name(config: &Config) -> String {
    format!("{}_dart_dispatch", config.cdylib_name())
}

/// Full native asset id the bindings resolve the companion under.
pub fn asset_id(config: &Config) -> String {
    format!("package:{}/{}_dispatch", config.package_name(), config.asset_id())
}

/// The companion's ABI hash symbol. Its name is unique to the library, so a
/// missing asset can't resolve to another package's companion through the
/// process-wide symbol fallback.
pub fn probe_symbol(config: &Config) -> String {
    format!("{}_relay_abi_hash", crate_name(config))
}

/// Symbol of the function that installs a callback type's proxy vtable.
pub fn install_symbol(namespace: &str, callback_name: &str) -> String {
    format!("uniffi_relay_{namespace}_install_{callback_name}")
}

fn vtable_name(callback_name: &str) -> String {
    format!("VTableCallbackInterface{callback_name}")
}

fn arguments(cb: &FfiCallbackFunction) -> Vec<String> {
    let mut args: Vec<_> = cb.arguments().iter().map(|a| native_type(&a.type_())).collect();
    if cb.has_rust_call_status_arg() {
        args.push("*mut RustCallStatus".into());
    }
    args
}

/// A callback type's vtable and how each of its slots is delivered.
struct Vtable<'s> {
    callback_name: String,
    table: &'s FfiStruct,
    slots: Vec<Slot>,
}

struct Slot {
    field: String,
    callback: String,
    delivery: Delivery,
}

struct Component<'a> {
    ci: &'a ComponentInterface,
    callbacks: BTreeMap<String, FfiCallbackFunction>,
    structs: BTreeMap<String, FfiStruct>,
}

impl<'a> Component<'a> {
    fn new(ci: &'a ComponentInterface) -> Self {
        let mut callbacks = BTreeMap::new();
        let mut structs = BTreeMap::new();
        for def in ci.ffi_definitions() {
            match def {
                FfiDefinition::CallbackFunction(cb) => {
                    callbacks.insert(cb.name().to_owned(), cb);
                }
                FfiDefinition::Struct(st) => {
                    structs.insert(st.name().to_owned(), st);
                }
                _ => {}
            }
        }
        Self { ci, callbacks, structs }
    }

    fn namespace(&self) -> &str {
        self.ci.namespace()
    }

    /// Each callback vtable with its fields' callback types and delivery.
    fn vtables(&self) -> Result<Vec<Vtable<'_>>> {
        let mut out = Vec::new();
        for (name, methods) in callback_types(self.ci) {
            let Some(table) = self.structs.get(&vtable_name(&name)) else {
                bail!("missing vtable for callback type `{name}`")
            };
            let mut slots = Vec::new();
            for field in table.fields() {
                let FfiType::Callback(callback) = field.type_() else {
                    bail!("invalid callback vtable field `{}`", field.name())
                };
                let delivery = delivery(field.name(), &methods)?;
                slots.push(Slot { field: field.name().to_owned(), callback, delivery });
            }
            out.push(Vtable { callback_name: name, table, slots });
        }
        Ok(out)
    }

    fn canonical(&self, out: &mut String) -> Result<()> {
        writeln!(out, "component {}", self.namespace())?;
        for (name, cb) in &self.callbacks {
            writeln!(
                out,
                "callback {name}: {}({})",
                result_type(cb.return_type()),
                arguments(cb).join(", ")
            )?;
        }
        for (name, st) in &self.structs {
            writeln!(out, "struct {name}")?;
            for field in st.fields() {
                writeln!(out, "  {}: {}", field.name(), native_type(&field.type_()))?;
            }
        }
        for vtable in self.vtables()? {
            writeln!(out, "install {}", install_symbol(self.namespace(), &vtable.callback_name))?;
            for slot in vtable.slots {
                writeln!(out, "  {}: {:?}", slot.field, slot.delivery)?;
            }
        }
        Ok(())
    }

    fn write(&self, source: &mut String) -> Result<()> {
        let ns = self.namespace();
        writeln!(source, "pub mod relay_{ns} {{\nuse super::*;")?;
        for (name, cb) in &self.callbacks {
            writeln!(
                source,
                "pub type {name} = unsafe extern \"C\" fn({}) -> {};",
                arguments(cb).join(", "),
                result_type(cb.return_type())
            )?;
        }
        for (name, st) in &self.structs {
            writeln!(source, "#[repr(C)] #[derive(Clone, Copy)] pub struct {name} {{")?;
            for field in st.fields() {
                writeln!(source, "pub r#{}: {},", field.name(), native_type(&field.type_()))?;
            }
            writeln!(source, "}}")?;
        }
        for vtable in self.vtables()? {
            self.write_vtable(source, &vtable)?;
        }
        writeln!(source, "}}")?;
        Ok(())
    }

    fn write_vtable(&self, source: &mut String, vtable_def: &Vtable<'_>) -> Result<()> {
        let name = &vtable_def.callback_name;
        let vtable = vtable_def.table.name();
        let slot = format!("SLOT_{name}");
        writeln!(
            source,
            "struct Slot{name} {{ ep: Arc<Endpoint>, inline: {vtable} }}
static {slot}: Mutex<Option<Arc<Slot{name}>>> = Mutex::new(None);
fn slot_{name}() -> Arc<Slot{name}> {{
    {slot}.lock().unwrap().clone().expect(\"relay vtable installed before use\")
}}"
        )?;
        for Slot { field, callback: cb_name, delivery } in &vtable_def.slots {
            let cb = &self.callbacks[cb_name];
            let args = arguments(cb);
            let params = (0..args.len()).map(|i| format!("a{i}: {}", args[i])).collect::<Vec<_>>();
            let all = (0..args.len()).map(|i| format!("a{i}")).collect::<Vec<_>>();
            let ret = result_type(cb.return_type());
            writeln!(
                source,
                "unsafe extern \"C\" fn proxy_{name}_{field}({}) -> {ret} {{
    let slot = slot_{name}();
    let f = slot.inline.r#{field};
    if slot.ep.on_owner_thread() {{
        return f({});
    }}
    let ep = slot.ep.clone();",
                params.join(", "),
                all.join(", ")
            )?;
            match delivery {
                Delivery::Wait => {
                    let tuple: String = all.iter().map(|a| format!("{a},")).collect();
                    let failed = if cb.has_rust_call_status_arg() {
                        format!("relay_failed(a{});", args.len() - 1)
                    } else {
                        String::new()
                    };
                    writeln!(
                        source,
                        "    let input = Transfer(({tuple}));
    match dispatch(&ep, move || {{
        let ({tuple}) = input.take();
        Transfer(unsafe {{ f({}) }})
    }}) {{
        Some(result) => result.take(),
        None => {{
            {failed}
            std::mem::zeroed()
        }}
    }}",
                        all.join(", ")
                    )?;
                }
                Delivery::Void => {
                    // The caller's out-return and status pointers die with its
                    // frame, so the queued call gets its own.
                    let kept = args.len() - 2;
                    let tuple: String = all[..kept].iter().map(|a| format!("{a},")).collect();
                    writeln!(
                        source,
                        "    let input = Transfer(({tuple}));
    ep.push(Job::with_abandon(
        move || {{
            let ({tuple}) = input.take();
            let mut scratch = [0u64; 4];
            let mut status: RustCallStatus = unsafe {{ std::mem::zeroed() }};
            unsafe {{ f({}scratch.as_mut_ptr().cast(), &mut status) }};
            if status.code != 0 {{
                warn_failed();
            }}
        }},
        warn_dropped,
    ));",
                        all[..kept].iter().map(|a| format!("{a}, ")).collect::<String>()
                    )?;
                }
                Delivery::Free => {
                    writeln!(
                        source,
                        "    ep.push(Job::with_abandon(move || unsafe {{ f(a0) }}, warn_dropped));"
                    )?;
                }
                Delivery::Async | Delivery::AsyncVoid => {
                    // The last three arguments are Rust's completion callback,
                    // its data, and an out-parameter in Rust's frame.
                    let n = args.len();
                    let complete = &cb.arguments()[n - 3];
                    let FfiType::Callback(complete_name) = complete.type_() else {
                        bail!("async callback `{cb_name}` has no completion callback")
                    };
                    let Some(FfiType::Struct(result_struct)) =
                        self.callbacks[&complete_name].arguments().get(1).map(|a| a.type_())
                    else {
                        bail!("completion callback `{complete_name}` has no result struct")
                    };
                    let FfiType::MutReference(out) = cb.arguments()[n - 1].type_() else {
                        bail!("async callback `{cb_name}` has no out-parameter")
                    };
                    let out = native_type(&out);
                    // A zeroed status is a success.
                    let failed = if *delivery == Delivery::AsyncVoid {
                        format!("let result: {result_struct} = unsafe {{ std::mem::zeroed() }};")
                    } else {
                        format!(
                            "let mut result: {result_struct} = unsafe {{ std::mem::zeroed() }};
            result.r#call_status.code = CALL_UNEXPECTED_ERROR;"
                        )
                    };
                    let tuple: String = all[..n - 1].iter().map(|a| format!("{a},")).collect();
                    let call: String = all[..n - 1].iter().map(|a| format!("{a}, ")).collect();
                    let (complete, data) = (n - 3, n - 2);
                    writeln!(
                        source,
                        "    let (complete, data) = (a{complete}, a{data});
    let input = Transfer(({tuple}));
    ep.push(Job::with_abandon(
        move || {{
            let ({tuple}) = input.take();
            // Only its address is passed: nothing reads or writes it, and
            // its function pointer field can't be zero-initialized.
            let mut scratch = std::mem::MaybeUninit::<{out}>::zeroed();
            unsafe {{ f({call}scratch.as_mut_ptr()) }};
        }},
        move || {{
            warn_dropped();
            {failed}
            unsafe {{ complete(data, result) }};
        }},
    ));"
                    )?;
                }
            }
            writeln!(source, "}}")?;
        }
        writeln!(source, "static PROXY_{name}: {vtable} = {vtable} {{")?;
        for slot in &vtable_def.slots {
            writeln!(source, "r#{0}: proxy_{name}_{0},", slot.field)?;
        }
        writeln!(source, "}};")?;
        writeln!(
            source,
            "#[no_mangle]
/// # Safety
/// `ep` must be the calling isolate's endpoint, and `inline` its vtable of
/// callbacks that run inline on that isolate's thread.
pub unsafe extern \"C\" fn {}(ep: u64, inline: *const {vtable}) -> *const {vtable} {{
    probe_endpoints();
    *{slot}.lock().unwrap() = Some(Arc::new(Slot{name} {{ ep: endpoint(ep), inline: *inline }}));
    &PROXY_{name}
}}",
            install_symbol(self.namespace(), name)
        )?;
        Ok(())
    }
}

/// Hash over the runtime and every relayed vtable. The bindings compare it
/// with the bundled companion's, so a stale companion fails loudly.
pub fn abi_hash(components: &[(&ComponentInterface, &Config)]) -> Result<u64> {
    let mut canonical = format!("relay-v1\n{CORE_RUNTIME}");
    for (ci, _) in components.iter().filter(|(ci, _)| has_callbacks(ci)) {
        Component::new(ci).canonical(&mut canonical)?;
    }
    Ok(fnv1a64(canonical.as_bytes()))
}

/// Writes the companion crate to `<out>/uniffi_dispatch`.
pub fn write_relay(
    components: &[(&ComponentInterface, &Config)],
    config: &Config,
    out: &Utf8Path,
) -> Result<()> {
    let mut source = CORE_RUNTIME.to_owned();
    writeln!(
        source,
        "#[no_mangle]\npub extern \"C\" fn {}() -> u64 {{ 0x{:016x} }}",
        probe_symbol(config),
        abi_hash(components)?
    )?;
    for (ci, _) in components.iter().filter(|(ci, _)| has_callbacks(ci)) {
        Component::new(ci).write(&mut source)?;
    }
    let dir = out.join("uniffi_dispatch");
    std::fs::create_dir_all(dir.join("src"))?;
    let crate_name = crate_name(config);
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"{crate_name}\"\nversion = \"0.0.0\"\nedition = \"2021\"\nrust-version = \"1.85\"\npublish = false\n[lib]\ncrate-type = [\"cdylib\"]\n[workspace]\n"
        ),
    )?;
    std::fs::write(dir.join("src/lib.rs"), source)?;
    std::fs::write(
        dir.join("README.md"),
        format!(
            "# Generated callback relay\n\nBuild this crate for the same targets as the original native library. It uses only the Rust standard library.\n\nLibrary basename: `{crate_name}`.\nNative asset ID: `{}`.\n\nBundle it with the original library so that Rust can call these bindings' callbacks from any thread, and so that an async Rust call that outlives its Dart isolate, for example across a hot restart, finishes without crashing. Without it, synchronous callbacks run only on the isolate's own thread. See docs/callback-threads.md in uniffi-dart.\n",
            asset_id(config)
        ),
    )?;
    Ok(())
}

/// A single-quoted Dart string literal for `text`.
fn dart_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('\'', "\\'").replace('$', "\\$");
    format!("'{escaped}'")
}

/// The shared runtime's relay section: the companion's asset and symbols, and
/// the per-isolate endpoint.
pub fn dart_runtime(config: &Config, hash: u64) -> String {
    include_str!("relay.dart.txt")
        .replace("@ASSET@", &asset_id(config))
        .replace("@PROBE@", &probe_symbol(config))
        .replace("@HASH@", &format!("0x{hash:016x}"))
}

/// The component's synchronous callback methods, as `Class.method`.
fn sync_methods(ci: &ComponentInterface) -> Vec<String> {
    use crate::gen::oracle::DartCodeOracle;
    callback_types(ci)
        .into_iter()
        .flat_map(|(name, methods)| {
            let class = DartCodeOracle::class_name(&name);
            methods
                .into_iter()
                .filter(|m| !m.is_async())
                .map(move |m| format!("{class}.{}", DartCodeOracle::fn_name(m.name())))
        })
        .collect()
}

/// The message a component logs once when it installs a vtable without the
/// companion, as a Dart string literal. `None` when it has no synchronous
/// callbacks, since listeners cover the rest.
pub fn missing_warning(ci: &ComponentInterface, config: &Config) -> Option<String> {
    let methods = sync_methods(ci);
    if methods.is_empty() {
        return None;
    }
    Some(dart_string(&format!(
        "uniffi-dart: the callback relay library is not bundled, so the synchronous \
         callbacks in {} can only run on this isolate's thread. A Rust call to any of \
         these from another thread aborts the process: {}. To allow it, build the \
         uniffi_dispatch crate generated with these bindings and bundle it as the \
         native asset {} (see docs/callback-threads.md in uniffi-dart).",
        ci.namespace(),
        methods.join(", "),
        asset_id(config)
    )))
}

/// Whether the component logs `missing_warning` when the companion is absent.
pub fn missing_warning_applies(ci: &ComponentInterface) -> bool {
    !sync_methods(ci).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(body: &str) -> ComponentInterface {
        let mut ci = ComponentInterface::from_webidl(body, "relay_test").unwrap();
        ci.derive_ffi_funcs().unwrap();
        ci
    }

    fn deliveries(ci: &ComponentInterface) -> Vec<(String, Delivery)> {
        Component::new(ci).vtables().unwrap()[0]
            .slots
            .iter()
            .map(|slot| (slot.field.clone(), slot.delivery))
            .collect()
    }

    #[test]
    fn only_calls_that_return_something_wait() {
        let ci = component(
            "namespace test {};
             [Error] enum Failure { \"Bad\" };
             callback interface Sink {
                 void note(u32 v);
                 u32 value();
                 [Throws=Failure] void check();
                 [Async] u32 later();
                 [Async] void note_later(u32 v);
                 [Async, Throws=Failure] void check_later();
             };",
        );
        let deliveries = deliveries(&ci);
        for (field, expected) in [
            // UniFFI 0.29's vtables have no uniffi_clone slot.
            ("uniffi_free", Delivery::Free),
            ("note", Delivery::Void),
            ("value", Delivery::Wait),
            ("check", Delivery::Wait),
            ("later", Delivery::Async),
            ("note_later", Delivery::AsyncVoid),
            ("check_later", Delivery::Async),
        ] {
            assert!(
                deliveries.contains(&(field.to_owned(), expected)),
                "{field} should be {expected:?}: {deliveries:?}"
            );
        }
    }

    #[test]
    fn components_without_sync_callbacks_have_no_warning() {
        let async_only =
            component("namespace test {}; callback interface Sink { [Async] u32 later(); };");
        assert!(has_callbacks(&async_only));
        assert!(!missing_warning_applies(&async_only));
        let sync = component("namespace test {}; callback interface Sink { u32 value(); };");
        assert!(missing_warning_applies(&sync));
        assert!(!has_callbacks(&component("namespace test { u32 value(); };")));
    }

    #[test]
    fn abi_hash_tracks_every_relayed_vtable() {
        let config = Config::default();
        let hash = |udl: &str| {
            let ci = component(udl);
            abi_hash(&[(&ci, &config)]).unwrap()
        };
        let base = "namespace test {}; callback interface Sink { u32 work(u32 v); };";
        assert_eq!(hash(base), hash(base));
        for changed in [
            "namespace test {}; callback interface Sink { u32 work(u16 v); };",
            "namespace test {}; callback interface Sink { void work(u32 v); };",
            "namespace test {}; callback interface Sink { u32 work(u32 v); u32 more(); };",
            "namespace test {}; callback interface Other { u32 work(u32 v); };",
        ] {
            assert_ne!(hash(base), hash(changed), "hash unchanged for {changed}");
        }
    }

    #[test]
    fn warning_is_a_safe_dart_literal() {
        let ci = component("namespace test {}; callback interface Sink { u32 value(); };");
        let warning = missing_warning(&ci, &Config::default()).unwrap();
        assert!(warning.starts_with('\'') && warning.ends_with('\''));
        assert!(warning.contains("Sink.value"));
        assert!(warning.contains("isolate\\'s thread"));
        assert!(!warning.contains('$') || warning.contains("\\$"));
    }
}
