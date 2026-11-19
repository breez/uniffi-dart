use genco::prelude::*;
use heck::ToUpperCamelCase;
use uniffi_bindgen::interface::ffi::{FfiStruct, FfiType};
use uniffi_bindgen::interface::{AsType, Method, Type};

use crate::gen::dispatch::relay;
use crate::gen::oracle::{AsCodeType, DartCodeOracle};
use crate::gen::render::{line_comment, AsRenderable, Renderable, TypeHelperRenderer};
use crate::gen::CodeType;

#[derive(Debug)]
pub struct CallbackInterfaceCodeType {
    name: String,
    self_type: Type,
}

impl CallbackInterfaceCodeType {
    pub fn new(name: String, self_type: Type) -> Self {
        Self { name, self_type }
    }
}

impl CodeType for CallbackInterfaceCodeType {
    fn type_label(&self) -> String {
        super::DartCodeOracle::class_name(&self.name)
    }

    fn canonical_name(&self) -> String {
        format!("CallbackInterface{}", self.type_label())
    }

    fn initialization_fn(&self) -> Option<String> {
        Some(format!("_uniffiInitializeCallbackInterface{}", self.name))
    }
}

impl Renderable for CallbackInterfaceCodeType {
    fn render_type_helper(&self, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
        type_helper.include_once_check(&self.canonical_name(), &self.self_type);
        let callback = type_helper.get_ci().get_callback_interface_definition(&self.name).unwrap();

        // Generate all necessary components for the callback interface
        let interface = generate_callback_interface(
            callback.name(),
            &callback.as_codetype().ffi_converter_name(),
            &callback.methods(),
            type_helper,
            None,
        );
        let vtable_interface =
            generate_callback_vtable_interface(callback.name(), &callback.methods());
        let functions =
            generate_callback_functions(callback.name(), &callback.methods(), type_helper);
        let namespace = type_helper
            .get_ci()
            .namespace_for_type(&callback.as_type())
            .unwrap_or_else(|_| type_helper.get_ci().namespace())
            .to_string();
        let ffi_module = DartCodeOracle::infer_ffi_module(type_helper.get_ci(), move || namespace);
        let vtable_init = generate_callback_interface_vtable_init_function(
            callback.name(),
            &callback.methods(),
            &ffi_module,
            type_helper,
        );

        quote! {
            $interface
            $vtable_interface
            $(callback_impl_class(callback.name(), functions, vtable_init))
        }
    }
}

pub fn generate_callback_interface(
    callback_name: &str,
    ffi_converter_name: &str,
    methods: &[&Method],
    type_helper: &dyn TypeHelperRenderer,
    rust_impl_name: Option<&str>,
) -> dart::Tokens {
    let cls_name = &DartCodeOracle::class_name(callback_name);
    let ffi_conv_name = &DartCodeOracle::class_name(ffi_converter_name);
    let impl_class = &callback_impl_class_name(callback_name);
    let lift_rust_impl =
        rust_impl_name.map(|name| quote!(return $name._internal(handle);)).unwrap_or_else(|| {
            quote!(
                throw UniffiInternalError(
                    UniffiInternalError.unexpectedStaleHandle,
                    "Rust-owned callback interface handle is not supported for $cls_name",
                );
            )
        });
    // UniFFI 0.29: Rust's `try_lift` for a `with_foreign` trait always treats the incoming
    // value as a foreign handle, so even Rust-backed wrappers must go through the handle map
    // (as Kotlin 0.29 does). Passing their Arc pointer would be misread as a handle.
    // A disposed wrapper is still rejected up front, as `uniffiClonePointer` does.
    let lower_rust_impl = rust_impl_name
        .map(|name| {
            quote!(
                if (value is $name && value._uniffiDisposed) {
                    throw StateError($(quoted(format!("Cannot use a disposed {cls_name}"))));
                }
            )
        })
        .unwrap_or_else(|| quote!());

    let mut async_struct_defs: Vec<dart::Tokens> = Vec::new();
    let mut async_completion_typedefs: Vec<dart::Tokens> = Vec::new();

    for method in methods {
        if method.is_async() {
            let struct_def = method.foreign_future_ffi_result_struct();
            let struct_name = struct_def.name().to_string();

            if !type_helper.include_once_by_name(&struct_name) {
                async_struct_defs.push(generate_foreign_future_struct_definition(&struct_def));
            }

            let completion_name = foreign_future_completion_name(method);
            if !type_helper.include_once_by_name(&completion_name) {
                async_completion_typedefs.push(generate_foreign_future_completion_typedef(
                    &completion_name,
                    &struct_name,
                ));
            }
        }
    }

    let async_support = if !async_struct_defs.is_empty() || !async_completion_typedefs.is_empty() {
        quote! {
            $(for typedef in &async_completion_typedefs => $typedef)
            $(for struct_def in &async_struct_defs => $struct_def)
        }
    } else {
        quote!()
    };

    let tokens = quote! {
        // This is the abstract class to be implemented
        abstract class $cls_name {
            $(for m in methods {
                $(generate_callback_methods_definitions(m, type_helper))
            })
        }

        // This is the type helper to convert from FFI to Dart
        class $ffi_conv_name {
            static final _handleMap = UniffiHandleMap<$cls_name>();
            static bool _vtableInitialized = false;

            static $cls_name lift(Pointer<Void> handle) {
                final rawHandle = handle.address;
                if ((rawHandle & 0x1) == 0) {
                    $lift_rust_impl
                }
                return _handleMap.remove(rawHandle);
            }

            static Pointer<Void> lower($cls_name value) {
                $(if type_helper.callback_dispatch() => _uniffiDispatch.id;)
                $lower_rust_impl
                _ensureVTableInitialized();
                final handle = _handleMap.insert(value);
                UniffiArgumentScope.current?.own((_) { _handleMap.remove(handle); });
                return Pointer<Void>.fromAddress($(if type_helper.callback_dispatch() { $(format!("_uniffiRegister{}", cls_name))(handle) } else { handle }));
            }

            static void _ensureVTableInitialized() {
                if (!_vtableInitialized) {
                    $impl_class.uniffiInit();
                    _vtableInitialized = true;
                }
            }

            static LiftRetVal<$cls_name> read(Uint8List buf) {
                final handle = buf.buffer.asByteData(buf.offsetInBytes).getInt64(0);
                final pointer = Pointer<Void>.fromAddress(handle);
                return LiftRetVal(lift(pointer), 8);
            }

            static int write($cls_name value, Uint8List buf) {
                final handle = lower(value);
                buf.buffer.asByteData(buf.offsetInBytes).setInt64(0, handle.address);
                return 8;
            }

            static int allocationSize($cls_name value) {
                return 8; // Just a handle (int64).
            }
        }

        // Additional support definitions for async callbacks
        $async_support

        // We must define callback signatures
        $(generate_callback_methods_signatures(cls_name, methods))
    };

    tokens
}

fn generate_callback_methods_definitions(
    method: &Method,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let method_name = DartCodeOracle::fn_name(method.name());
    let dart_args = &method
        .arguments()
        .iter()
        .map(|arg| {
            let arg_type = arg.as_renderable().render_type(&arg.as_type(), type_helper);
            let arg_name = DartCodeOracle::var_name(arg.name());

            quote!($arg_type $arg_name)
        })
        .collect::<Vec<_>>();

    // A result is declared in its stored form (BigInt for u64), not the
    // int-or-BigInt input form, so implementers keep static type checking.
    // Lowering still range-checks the value before it reaches Rust.
    let ret_type = if method.is_async() {
        if let Some(ret) = method.return_type() {
            let rendered = ret.as_renderable().render_type(ret, type_helper);
            quote!(Future<$rendered>)
        } else {
            quote!(Future<void>)
        }
    } else if let Some(ret) = method.return_type() {
        ret.as_renderable().render_type(ret, type_helper)
    } else {
        quote!(void)
    };

    quote!(
        $ret_type $method_name($(for a in dart_args => $a,));
    )
}

fn generate_callback_methods_signatures(callback_name: &str, methods: &[&Method]) -> dart::Tokens {
    let mut tokens = dart::Tokens::new();
    for (method_index, method) in methods.iter().enumerate() {
        //let method_name = DartCodeOracle::fn_name(method.name());

        let ffi_method_type = format!("UniffiCallbackInterface{callback_name}Method{method_index}");
        let dart_method_type =
            format!("UniffiCallbackInterface{callback_name}Method{method_index}Dart");

        let arg_native_types: Vec<dart::Tokens> = method
            .arguments()
            .iter()
            .map(|arg| DartCodeOracle::native_type_label(Some(&arg.as_type())))
            .collect();

        let arg_dart_types: Vec<dart::Tokens> = method
            .arguments()
            .iter()
            .map(|arg| DartCodeOracle::native_dart_type_label(Some(&arg.as_type())))
            .collect();

        if method.is_async() {
            let completion_base = foreign_future_completion_name(method);
            let completion_native = format!("Uniffi{}", completion_base.to_upper_camel_case());
            let completion_pointer = format!("Pointer<NativeFunction<{}>>", completion_native);

            tokens.append(quote! {
                typedef $ffi_method_type = Void Function(
                    Uint64, $(for arg in &arg_native_types => $arg,)
                    $(&completion_pointer), Uint64, Pointer<UniffiForeignFuture>);
                typedef $dart_method_type = void Function(
                    int, $(for arg in &arg_dart_types => $arg,)
                    $(&completion_pointer), int, Pointer<UniffiForeignFuture>);
            });
        } else {
            let method_return_type = if let Some(ret) = method.return_type() {
                DartCodeOracle::native_type_label(Some(ret))
            } else {
                quote!(Void)
            };

            tokens.append(quote! {
                typedef $ffi_method_type = Void Function(
                    Uint64, $(for arg in &arg_native_types => $arg,)
                    Pointer<$(&method_return_type)>, Pointer<RustCallStatus>);
                typedef $dart_method_type = void Function(
                    int, $(for arg in &arg_dart_types => $arg,)
                    Pointer<$(&method_return_type)>, Pointer<RustCallStatus>);
            });
        }
    }

    tokens.append(quote! {
        typedef UniffiCallbackInterface$(callback_name)Free = Void Function(Uint64);
        typedef UniffiCallbackInterface$(callback_name)FreeDart = void Function(int);
    });

    tokens
}

pub fn generate_callback_vtable_interface(
    callback_name: &str,
    methods: &[&Method],
) -> dart::Tokens {
    let vtable_name = format!("UniffiVTableCallbackInterface{callback_name}");
    let methods_vec: Vec<_> = methods.iter().enumerate().collect();

    // UniFFI 0.29 vtable layout: one field per method, then `uniffi_free`.
    // There is no `uniffi_clone` slot (that, and free-first ordering, arrived in 0.30).
    quote! {
        final class $vtable_name extends Struct {
            $(for (index, m) in &methods_vec =>
                external Pointer<NativeFunction<UniffiCallbackInterface$(callback_name)Method$(format!("{}",index))>> $(DartCodeOracle::fn_name(m.name()));
            )
            external Pointer<NativeFunction<UniffiCallbackInterface$(callback_name)Free>> uniffiFree;
        }
    }
}

pub fn generate_callback_functions(
    callback_name: &str,
    methods: &[&Method],
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let cls_name = &DartCodeOracle::class_name(callback_name);
    // `callback_dispatch` relays the free through its companion like every
    // other synchronous callback, so it keeps `Pointer.fromFunction`. It
    // rejects async methods.
    let listeners = !type_helper.callback_dispatch();

    let functions: Vec<dart::Tokens> = methods.iter().enumerate().map(|(index, m)| {
        let method_name = &DartCodeOracle::fn_name(m.name()).to_string();
        let ffi_method_type = &format!("UniffiCallbackInterface{callback_name}Method{index}");
        let _dart_method_type = &format!("UniffiCallbackInterface{callback_name}Method{index}Dart");

        // Get parameter types using the oracle
        let param_types: Vec<dart::Tokens> = m
            .arguments()
            .iter()
            .enumerate()
            .map(|(index, arg)| {
                let arg_name = format!("_uniffiRaw{index}");
                DartCodeOracle::callback_param_type(&arg.as_type(), &arg_name)
            })
            .collect();

        // Get argument lifts using the oracle
        let arg_lifts: Vec<dart::Tokens> = m
            .arguments()
            .iter()
            .enumerate()
            .map(|(arg_idx, arg)| {
                let arg_name = format!("_uniffiRaw{arg_idx}");
                DartCodeOracle::callback_arg_lift_indexed(&arg.as_type(), &arg_name, arg_idx)
            })
            .collect();

        // The callback boundary owns every incoming serialized argument, including
        // those not reached if an earlier argument fails to lift.
        let free_buffers = quote!($(for (index, _) in m.arguments().iter().enumerate().filter(|(_, arg)| matches!(FfiType::from(&arg.as_type()), FfiType::RustBuffer(_))) =>
            uniffiFreeRustBuffer($(format!("_uniffiRaw{index}")));
        ));

        // Prepare arg names for the method call using indexes
        let arg_names: Vec<dart::Tokens> = m
            .arguments()
            .iter()
            .enumerate()
            .map(|(arg_idx, arg)| DartCodeOracle::callback_arg_name(&arg.as_type(), arg_idx))
            .collect();

        // Generate the function body
        // Static members of the callback type's impl class, named after the
        // method with a suffix, so they can't collide with exported functions,
        // other callback types, or each other.
        let callback_method_name = &format!("{}Callback", DartCodeOracle::fn_name(m.name()));
        let method_pointer = &format!("{}Pointer", DartCodeOracle::fn_name(m.name()));

        if m.is_async() {
            let completion_base = foreign_future_completion_name(m);
            let completion_native = format!("Uniffi{}", completion_base.to_upper_camel_case());
            let completion_pointer = format!("Pointer<NativeFunction<{}>>", completion_native);
            let completion_dart = format!("{completion_native}Dart");
            let result_struct = m.foreign_future_ffi_result_struct();
            let struct_tokens = DartCodeOracle::ffi_struct_name(result_struct.name());
            let struct_tokens_alt = struct_tokens.clone();

            if let Some(ret) = m.return_type() {
                type_helper.include_once_check(&ret.as_codetype().canonical_name(), ret);
            }

            // Flexible u64 results are validated by this lowering. It runs before
            // completion is claimed, so a rejected value reaches the catch below
            // and the future still completes with an error.
            let success_return = if let Some(ret) = m.return_type() {
                let converter = ret.as_codetype().ffi_converter_name();
                let value = super::unsigned::normalize_for_lower(ret, quote!(result), type_helper);
                quote!(resultStruct.returnValue = $(&converter).lower($value);)
            } else {
                quote!()
            };

            let async_error_handling = callback_error_handling(
                m.throws_type(),
                type_helper,
                quote!(resultStruct.callStatus),
            );

            quote! {
                $(line_comment(&[
                    "Runs as a NativeCallable.listener, on this isolate's event loop after",
                    "Rust's call has returned. It must not write `outReturn`, which points",
                    "into Rust's stack frame. Rust pre-fills it with a no-op free callback.",
                    "The Dart work always runs to completion. If Rust has stopped waiting,",
                    "it discards the result.",
                ]))
                static void $callback_method_name(
                    int uniffiHandle,
                    $(for param in &param_types => $param,)
                    $(&completion_pointer) uniffiFutureCallback,
                    int uniffiCallbackData,
                    Pointer<UniffiForeignFuture> outReturn,
                ) {
                    try {
                    final callback = uniffiFutureCallback.asFunction<$(&completion_dart)>();

                    () async {
                        var completed = false;
                        try {
                            // The handle lookup and argument lifts run inside
                            // the guarded body: a stale handle or a failing
                            // lift must still complete the native future.
                            $(handle_lookup(cls_name, m, quote! {
                                final resultStruct = Struct.create<$(&struct_tokens)>();
                                resultStruct.callStatus.code = CALL_SUCCESS;
                                completed = true;
                                callback(uniffiCallbackData, resultStruct);
                                return;
                            }))
                            $(arg_lifts)
                            final result = await obj.$method_name($(for arg in &arg_names => $arg,));
                            if (completed) {
                                return;
                            }
                            final resultStruct = Struct.create<$struct_tokens>();
                            {
                                $success_return
                                resultStruct.callStatus.code = CALL_SUCCESS;
                                // Conversion can throw. Claim completion only once the
                                // result/status is ready to transfer to Rust.
                                completed = true;
                                callback(uniffiCallbackData, resultStruct);
                            }
                        } catch (e) {
                            if (completed) {
                                return;
                            }
                            final resultStruct = Struct.create<$struct_tokens_alt>();
                            {
                                $async_error_handling
                                // Conversion can throw. Claim completion only once the
                                // result/status is ready to transfer to Rust.
                                completed = true;
                                callback(uniffiCallbackData, resultStruct);
                            }
                        }
                    }();
                    } finally {
                        $free_buffers
                    }
                }

                $(callback_registration(callback_method_name, method_pointer, ffi_method_type, true))
                $(if listeners {
                    static final Pointer<NativeFunction<$ffi_method_type>> $(format!("{}InlinePointer", DartCodeOracle::fn_name(m.name()))) =
                        Pointer.fromFunction<$ffi_method_type>($callback_method_name);
                })
            }
        } else {
            let call_status = &DartCodeOracle::call_status_name(&m.arguments());
            // Handle return value using the oracle
            let call_dart_method = if let Some(ret) = m.return_type() {
                DartCodeOracle::callback_return_handling(
                    ret,
                    method_name,
                    arg_names,
                    call_status,
                    type_helper,
                )
            } else {
                // Handle void return types
                DartCodeOracle::callback_void_handling(method_name, arg_names, call_status)
            };

            // Get the appropriate out return type
            let out_return_type = DartCodeOracle::callback_out_return_type(m.return_type());
            let sync_error_handling =
                callback_error_handling(m.throws_type(), type_helper, quote!($call_status));

            quote! {
                static void $callback_method_name(int uniffiHandle, $(for param in &param_types => $param,) $out_return_type outReturn, Pointer<RustCallStatus> callStatus) {
                    final $call_status = callStatus.ref;
                    try {
                        $(handle_lookup(cls_name, m, quote! {
                            $call_status.code = CALL_SUCCESS;
                            return;
                        }))
                        $(arg_lifts)
                        $call_dart_method
                    } catch (e) {
                        $sync_error_handling
                    } finally {
                        $free_buffers
                    }
                }

                $(callback_registration(callback_method_name, method_pointer, ffi_method_type, false))
            }
        }
    }).collect();

    // Free callback
    let free_callback_fn = "uniffiFree";
    let free_callback_pointer = "uniffiFreePointer";
    let free_callback_type = &format!("UniffiCallbackInterface{callback_name}Free");

    quote! {
        $(functions)

        static void $free_callback_fn(int handle) {
            try {
                FfiConverterCallbackInterface$cls_name._handleMap.remove(handle);
            } catch (e) {
                // Optionally log error, but do not return anything.
            }
        }

        $(callback_registration(free_callback_fn, free_callback_pointer, free_callback_type, listeners))
        $(if listeners {
            static final Pointer<NativeFunction<$free_callback_type>> uniffiFreeInlinePointer =
                Pointer.fromFunction<$free_callback_type>($free_callback_fn);
        })

    }
}

/// Whether Rust may see a call that can't be delivered succeed instead of fail:
/// the method returns nothing and can't throw. For such a method, UniFFI turns
/// an unexpected error into a Rust panic, which aborts a `panic = "abort"`
/// build. Dropping the call loses nothing that Rust could act on.
pub fn droppable(method: &Method) -> bool {
    method.return_type().is_none() && method.throws_type().is_none()
}

/// Looks up the callback object for `uniffiHandle` as `obj`. A handle this
/// isolate doesn't have belongs to an isolate that has exited, for example
/// before a hot restart (see `_uniffiHandlePrefix`). For a droppable method
/// the call is dropped through `on_missing`, which must report success to Rust
/// and return. Other methods throw a stale-handle error, which Rust receives as
/// an unexpected error.
fn handle_lookup(cls_name: &str, method: &Method, on_missing: dart::Tokens) -> dart::Tokens {
    let map = quote!(FfiConverterCallbackInterface$cls_name._handleMap);
    if droppable(method) {
        let label = format!("{cls_name}.{}", DartCodeOracle::fn_name(method.name()));
        quote! {
            final obj = $(&map).maybeGet(uniffiHandle);
            if (obj == null) {
                uniffiDropCallback($(quoted(label)));
                $on_missing
            }
        }
    } else {
        quote!(final obj = $map.get(uniffiHandle);)
    }
}

/// Exposes a generated callback function as a native function pointer.
///
/// A `NativeCallable.listener` may be invoked from any thread. The call is queued
/// and runs later on this isolate's event loop. `keepIsolateAlive = false` lets a
/// program exit while Rust still holds the pointer. `Pointer.fromFunction` runs
/// synchronously, but only on this isolate's own thread.
fn callback_registration(
    function_name: &str,
    pointer_name: &str,
    native_type: &str,
    listener: bool,
) -> dart::Tokens {
    if listener {
        let callable = &format!("_{pointer_name}Listener");
        quote! {
            static final NativeCallable<$native_type> $callable =
                NativeCallable<$native_type>.listener($function_name)..keepIsolateAlive = false;
            static final Pointer<NativeFunction<$native_type>> $pointer_name = $callable.nativeFunction;
        }
    } else {
        quote! {
            static final Pointer<NativeFunction<$native_type>> $pointer_name =
                Pointer.fromFunction<$native_type>($function_name);
        }
    }
}

fn callback_error_handling(
    throws_type: Option<&Type>,
    type_helper: &dyn TypeHelperRenderer,
    status: dart::Tokens,
) -> dart::Tokens {
    let encode = if let Some(error_type) = throws_type {
        let error_type_label = error_type.as_renderable().render_type(error_type, type_helper);
        let error_lower = error_type.as_codetype().ffi_converter_name();
        quote! {
            if (e is $error_type_label) {
                final buffer = $error_lower.lower(e);
                $(&status).errorBuf = buffer;
                $(&status).code = CALL_ERROR;
            } else {
                $(&status).errorBuf = FfiConverterString.lower(e.toString());
            }
        }
    } else {
        quote!($(&status).errorBuf = FfiConverterString.lower(e.toString());)
    };
    quote! {
        $(&status).code = CALL_UNEXPECTED_ERROR;
        try {
            $encode
        } catch (_) {
            $(&status).code = CALL_UNEXPECTED_ERROR;
            try {
                $(&status).errorBuf = FfiConverterString.lower(
                    "uniffi: callback error could not be encoded");
            } catch (_) {
                $(&status).errorBuf = Struct.create<RustBuffer>();
            }
        }
    }
}

fn generate_foreign_future_struct_definition(ffi_struct: &FfiStruct) -> dart::Tokens {
    let struct_name = DartCodeOracle::ffi_struct_name(ffi_struct.name());
    let fields: Vec<dart::Tokens> = ffi_struct
        .fields()
        .iter()
        .map(|field| {
            let field_name = DartCodeOracle::var_name(field.name());
            let ffi_field_type = field.type_();
            let field_type = match &ffi_field_type {
                FfiType::RustCallStatus => quote!(RustCallStatus),
                _ => DartCodeOracle::ffi_dart_type_label(Some(&ffi_field_type)),
            };
            if let Some(annotation) = foreign_future_field_annotation(&ffi_field_type) {
                quote! {
                    $annotation
                    external $field_type $field_name;
                }
            } else {
                quote! {
                    external $field_type $field_name;
                }
            }
        })
        .collect();

    quote! {
        final class $struct_name extends Struct {
            $(for field in fields => $field)
        }
    }
}

fn generate_foreign_future_completion_typedef(
    callback_base: &str,
    struct_name: &str,
) -> dart::Tokens {
    let native_callback_name = format!("Uniffi{}", callback_base.to_upper_camel_case());
    let dart_callback_name = format!("{native_callback_name}Dart");
    let struct_tokens = DartCodeOracle::ffi_struct_name(struct_name);
    let struct_tokens_alt = struct_tokens.clone();

    let mut tokens = dart::Tokens::new();
    tokens.append(quote!(typedef $native_callback_name = Void Function(Uint64, $struct_tokens);));
    tokens.append(quote!(typedef $dart_callback_name = void Function(int, $struct_tokens_alt);));
    tokens
}

fn foreign_future_completion_name(method: &Method) -> String {
    let return_ffi_type = method.return_type().cloned().map(FfiType::from);
    let suffix = FfiType::return_type_name(return_ffi_type.as_ref()).to_upper_camel_case();
    format!("ForeignFutureComplete{suffix}")
}

fn foreign_future_field_annotation(field_type: &FfiType) -> Option<dart::Tokens> {
    match field_type {
        FfiType::Int8 => Some(quote!(@Int8())),
        FfiType::UInt8 => Some(quote!(@Uint8())),
        FfiType::Int16 => Some(quote!(@Int16())),
        FfiType::UInt16 => Some(quote!(@Uint16())),
        FfiType::Int32 => Some(quote!(@Int32())),
        FfiType::UInt32 => Some(quote!(@Uint32())),
        FfiType::Int64 => Some(quote!(@Int64())),
        FfiType::UInt64 | FfiType::Handle => Some(quote!(@Uint64())),
        FfiType::Float32 => Some(quote!(@Float())),
        FfiType::Float64 => Some(quote!(@Double())),
        _ => None,
    }
}

pub fn generate_callback_interface_vtable_init_function(
    callback_name: &str,
    methods: &[&Method],
    ffi_module: &str,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let callback_dispatch = type_helper.callback_dispatch();
    let vtable_name = &format!("UniffiVTableCallbackInterface{callback_name}");
    let vtable_static_instance_name = "uniffiVTable";
    let snake_callback = callback_name.to_lowercase();
    let cls_name = &DartCodeOracle::class_name(callback_name);
    // `inline` picks the Pointer.fromFunction variant of each listener, for the
    // relay to call on the owning isolate's thread.
    let fill = |target: &str, inline: bool| -> dart::Tokens {
        let suffix = |listener: bool| if inline && listener { "InlinePointer" } else { "Pointer" };
        quote! {
            $target.ref.uniffiFree = $(format!("uniffiFree{}", suffix(true)));
            $(for m in methods {
                $target.ref.$(DartCodeOracle::fn_name(m.name())) = $(format!("{}{}", DartCodeOracle::fn_name(m.name()), suffix(m.is_async())));
            })
        }
    };
    let init_call = quote! {
        rustCall((status) {
            uniffi_$(ffi_module)_fn_init_callback_vtable_$(snake_callback)(
                $vtable_static_instance_name,
            );
            checkCallStatus(NullRustCallStatusErrorHandler(), status);
        });
    };

    if callback_dispatch {
        return quote! {
            static late final Pointer<$vtable_name> $vtable_static_instance_name;

            static void uniffiInit() {
                // Make initialization idempotent - return early if already initialized
                if (FfiConverterCallbackInterface$cls_name._vtableInitialized) {
                    return;
                }

                $vtable_static_instance_name = calloc<$vtable_name>();
                $(fill(vtable_static_instance_name, false))

                $init_call

                calloc.free($vtable_static_instance_name);

                // Update the flag to prevent re-initialization
                FfiConverterCallbackInterface$cls_name._vtableInitialized = true;
            }
        };
    }

    let ci = type_helper.get_ci();
    let install_symbol = relay::install_symbol(ci.namespace(), callback_name);
    let install_fn = &format!("_uniffiRelayInstall{callback_name}");
    let warn = relay::missing_warning_applies(ci);
    quote! {
        static late final Pointer<$vtable_name> $vtable_static_instance_name;

        @Native<Pointer<$vtable_name> Function(Uint64, Pointer<$vtable_name>)>(
            assetId: uniffiCallbackRelayAsset,
            symbol: $(quoted(install_symbol)),
        )
        external static Pointer<$vtable_name> $install_fn(int endpoint, Pointer<$vtable_name> inline);

        static void uniffiInit() {
            // Make initialization idempotent - return early if already initialized
            if (FfiConverterCallbackInterface$cls_name._vtableInitialized) {
                return;
            }

            final relay = UniffiCallbackRelay.endpoint;
            if (relay != null) {
                $(line_comment(&[
                    "The relay copies these and gives Rust its own proxies, which call",
                    "them inline on this isolate's thread and queue calls from any other.",
                ]))
                final inline = calloc<$vtable_name>();
                $(fill("inline", true))
                $vtable_static_instance_name = $install_fn(relay, inline);
                calloc.free(inline);
            } else {
                $(if warn => UniffiCallbackRelay.warnMissing($(quoted(ci.namespace())), _uniffiCallbackRelayMissing);)
                $vtable_static_instance_name = calloc<$vtable_name>();
                $(fill(vtable_static_instance_name, false))
            }

            $init_call

            // Update the flag to prevent re-initialization
            FfiConverterCallbackInterface$cls_name._vtableInitialized = true;
        }
    }
}

/// The class that holds a callback type's generated trampolines, function
/// pointers and vtable, so none of them is a top-level name. Top-level
/// `{trait}{Method}` functions used to collide with exported functions and
/// with other callback types' functions. The suffix keeps the class name apart
/// from the `UniffiCallbackInterface{Name}Free`-style typedefs.
pub fn callback_impl_class_name(callback_name: &str) -> String {
    format!("UniffiCallbackInterface{}Impl", DartCodeOracle::class_name(callback_name))
}

/// Wraps the callback functions and vtable setup in the impl class.
pub fn callback_impl_class(
    callback_name: &str,
    functions: dart::Tokens,
    vtable_init: dart::Tokens,
) -> dart::Tokens {
    let class = &callback_impl_class_name(callback_name);
    quote! {
        abstract final class $class {
            $functions

            $vtable_init
        }
    }
}
