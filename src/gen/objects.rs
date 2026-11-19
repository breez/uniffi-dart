use std::fmt::Debug;

use genco::prelude::*;
use heck::ToLowerCamelCase;
use uniffi_bindgen::interface::{AsType, Method, Object, ObjectImpl, UniffiTrait};
use uniffi_bindgen::pipeline::general::nodes::Literal;

use super::defaults::{lower_argument, render_argument_param};
use super::stream::generate_stream;
use crate::gen::callback_interface::{
    callback_impl_class, generate_callback_functions, generate_callback_interface,
    generate_callback_interface_vtable_init_function, generate_callback_vtable_interface,
};
use crate::gen::dispatch::relay;
use crate::gen::oracle::{AsCodeType, DartCodeOracle};
use crate::gen::render::{AsRenderable, Renderable, TypeHelperRenderer};
use crate::gen::CodeType;

#[derive(Debug)]
pub struct ObjectCodeType {
    id: String,
    imp: ObjectImpl,
}

impl ObjectCodeType {
    pub fn new(id: String, imp: ObjectImpl) -> Self {
        Self { id, imp }
    }
}

impl CodeType for ObjectCodeType {
    fn type_label(&self) -> String {
        DartCodeOracle::class_name(&self.id)
    }

    fn canonical_name(&self) -> String {
        self.type_label()
    }

    fn literal(&self, _literal: &Literal) -> String {
        unreachable!();
    }

    fn ffi_converter_name(&self) -> String {
        match self.imp {
            // The converter lives in its own class rather than on the object itself, so
            // that a Rust method named `read`/`write`/`lower` cannot collide with the
            // converter's statics (Dart rejects a static and an instance member sharing
            // a name). Matches how records, enums and callback interfaces are emitted.
            ObjectImpl::Struct | ObjectImpl::Trait => {
                format!("FfiConverter{}", self.canonical_name())
            }
            ObjectImpl::CallbackTrait => format!("FfiConverterCallbackInterface{}", self.id),
        }
    }
}

impl Renderable for ObjectCodeType {
    fn render_type_helper(&self, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
        if type_helper.check(&self.id) {
            quote!()
        } else if let Some(obj) = type_helper.get_object(&self.id) {
            generate_object(obj, type_helper)
        } else {
            unreachable!()
        }
    }
}
pub fn generate_object(obj: &Object, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    type_helper.include_once_check(obj.name(), &obj.as_type());

    if obj.has_callback_interface() {
        let interface = generate_callback_interface(
            obj.name(),
            &obj.as_codetype().ffi_converter_name(),
            &obj.methods(),
            type_helper,
            Some(&format!("_{}Impl", DartCodeOracle::class_name(obj.name()))),
        );
        let vtable_interface = generate_callback_vtable_interface(obj.name(), &obj.methods());
        let functions = generate_callback_functions(obj.name(), &obj.methods(), type_helper);
        let fallback_namespace = {
            let namespace = type_helper
                .get_ci()
                .namespace_for_type(&obj.as_type())
                .expect("object should have namespace");
            namespace.to_string()
        };
        let ffi_module =
            DartCodeOracle::infer_ffi_module(type_helper.get_ci(), move || fallback_namespace);
        let vtable_init = generate_callback_interface_vtable_init_function(
            obj.name(),
            &obj.methods(),
            &ffi_module,
            type_helper,
        );
        let rust_impl = generate_callback_trait_rust_impl(obj, type_helper);
        return quote!(
            $interface
            $rust_impl
            $vtable_interface
            $(callback_impl_class(obj.name(), functions, vtable_init))
        );
    } else if obj.is_trait_interface() {
        return generate_trait_object(obj, type_helper);
    }

    let cls_name = &DartCodeOracle::class_name(obj.name());
    let ffi_converter_name = &obj.as_codetype().ffi_converter_name();
    let interface_name = DartCodeOracle::object_interface_name(type_helper.get_ci(), obj);
    let interface_definition = generate_object_interface(obj, &interface_name, type_helper);
    let finalizer_cls_name = &format!("{cls_name}Finalizer");
    let ffi_object_free_name = obj.ffi_object_free().name();
    let ffi_object_clone_name = obj.ffi_object_clone().name();

    // Stream workaround, make it more elegant later

    let stream_glue =
        if obj.name().contains("StreamExt") { generate_stream(obj, type_helper) } else { quote!() };

    let mut constructor_definitions: Vec<dart::Tokens> = Vec::new();
    let mut async_constructor_factories: Vec<dart::Tokens> = Vec::new();

    for constructor in obj.constructors() {
        let ffi_func_name = constructor.ffi_func().name();
        let constructor_name = constructor.name();

        let dart_constructor_decl = if constructor_name == "new" {
            quote!($cls_name)
        } else {
            quote!($cls_name.$(DartCodeOracle::fn_name(constructor_name)))
        };

        // Check if function can throw errors
        let error_handler = if let Some(error_type) = constructor.throws_type() {
            let error_name =
                DartCodeOracle::class_name(error_type.name().unwrap_or("UnknownError"));
            // Use the consistent Exception naming for error handlers
            let handler_name = format!("{}ErrorHandler", error_name.to_lower_camel_case());
            quote!($(handler_name))
        } else {
            quote!(null)
        };

        let dart_params = if constructor.arguments().is_empty() {
            quote!()
        } else {
            quote!({$(for arg in constructor.arguments() =>
                $(render_argument_param(
                    arg,
                    quote!($(DartCodeOracle::dart_type_label(Some(&arg.as_type())))),
                    type_helper,
                )),
            )})
        };

        let call_status = &DartCodeOracle::call_status_name(&constructor.arguments());
        // Configured custom types apply their defaults while lowering.
        let lowered: Vec<_> =
            constructor.arguments().iter().map(|arg| lower_argument(arg, type_helper)).collect();
        let call = DartCodeOracle::scoped_ffi_call(
            ffi_func_name,
            &constructor.arguments(),
            false,
            if constructor.is_async() { None } else { Some(quote!($call_status)) },
            Some(&lowered),
            type_helper,
        );

        // Ensure argument types are included
        for arg in constructor.arguments() {
            type_helper.include_once_check(&arg.as_codetype().canonical_name(), &arg.as_type());
        }

        if constructor.is_async() {
            async_constructor_factories.push(quote! {
                static Future<$cls_name> $(DartCodeOracle::fn_name(constructor_name))($dart_params) {
                    return uniffiRustCallAsync(
                      () => $(&call),
                      $(DartCodeOracle::async_poll(constructor, type_helper.get_ci())),
                      $(DartCodeOracle::async_complete(constructor, type_helper.get_ci())),
                      $(DartCodeOracle::async_free(constructor, type_helper.get_ci())),
                      (Pointer<Void> ptr) => $cls_name._(ptr),
                      $error_handler,
                      $(DartCodeOracle::async_finalizer(constructor, type_helper)),
                    );
                }
            });
        } else {
            constructor_definitions.push(quote! {
                // Public constructor
                $dart_constructor_decl($dart_params) : _ptr = rustCall(($call_status) =>
                    $(&call),
                    $error_handler
                ) {
                     _$finalizer_cls_name.attach(this, _ptr, detach: this);
                }
            });
        }
    }

    // For interface objects that are used as error types, generate error handlers
    let is_error_interface = type_helper.get_ci().is_name_used_as_error(obj.name());

    let error_handler_class = if is_error_interface {
        // Generate error handlers for specific error interfaces
        let error_handler_name = format!("{cls_name}ErrorHandler");
        let instance_name = cls_name.to_lower_camel_case();
        quote! {
            class $(&error_handler_name) extends UniffiRustCallStatusErrorHandler {
                @override
                Exception lift(RustBuffer errorBuf) {
                    return $(ffi_converter_name).read(errorBuf.asUint8List()).value;
                }
            }

            final $(&error_handler_name) $(instance_name)ErrorHandler = $(&error_handler_name)();
        }
    } else {
        quote!()
    };

    let mut implements: Vec<String> = Vec::new();
    if !obj.is_trait_interface() {
        implements.push(interface_name.clone());
    }
    // A NativeFinalizer can only watch a Finalizable.
    implements.push("Finalizable".to_string());
    if is_error_interface && !implements.iter().any(|entry| entry == "Exception") {
        implements.push("Exception".to_string());
    }

    for trait_impl in obj.trait_impls() {
        let trait_iface =
            DartCodeOracle::trait_interface_name(type_helper.get_ci(), &trait_impl.trait_name);
        if !implements.contains(&trait_iface) {
            implements.push(trait_iface);
        }
    }

    let implements_clause = if implements.is_empty() {
        quote!()
    } else {
        quote!( implements $(for imp in implements.iter() join (, ) => $(imp)))
    };

    // Generate toString() method for error interfaces
    let has_display_trait =
        obj.uniffi_traits().iter().any(|t| matches!(t, UniffiTrait::Display { .. }));

    let to_string_method: dart::Tokens =
        if is_error_interface && !obj.is_trait_interface() && !has_display_trait {
            // Only generate toString for regular error interfaces, skip trait interfaces for now
            let dart_class_name = format!("\"{cls_name}\"");
            quote! {
                @override
                String toString() {
                    return $(&dart_class_name);
                }
            }
        } else {
            quote!()
        };

    let trait_methods = generate_trait_helpers(obj, type_helper);
    let (finalizer_externals, finalizer) = object_finalizer(obj, type_helper);

    quote! {
        $interface_definition

        $finalizer_externals
        final _$finalizer_cls_name = $finalizer;

        class $cls_name $implements_clause {
            late final Pointer<Void> _ptr;
            bool _uniffiDisposed = false;

            // Private constructor for internal use / lift
            $cls_name._(this._ptr) {
                _$finalizer_cls_name.attach(this, _ptr, detach: this);
            }

            // Public constructors generated from UDL
            $( for ctor_def in constructor_definitions => $ctor_def )
            $( for factory_def in async_constructor_factories => $factory_def )

            // Factory for lifting pointers
            factory $cls_name.lift(Pointer<Void> ptr) {
                return $cls_name._(ptr);
            }

            Pointer<Void> uniffiClonePointer() {
                if (_uniffiDisposed) {
                    throw StateError($(quoted(format!("Cannot use a disposed {cls_name}"))));
                }
                final pointer = rustCall((status) => $ffi_object_clone_name(_ptr, status));
                UniffiArgumentScope.current?.own((status) => $ffi_object_free_name(pointer, status));
                return pointer;
            }

            void dispose() {
                if (_uniffiDisposed) {
                    return;
                }
                // Reserve call-status storage before committing the closed
                // state, so an allocation failure leaves disposal retryable.
                final status = calloc<RustCallStatus>();
                // Mark closed before calling Rust, including reentrant disposal.
                _uniffiDisposed = true;
                _$finalizer_cls_name.detach(this);
                try {
                    $ffi_object_free_name(_ptr, status);
                    checkCallStatus(NullRustCallStatusErrorHandler(), status);
                } finally {
                    calloc.free(status);
                }
            }

            $to_string_method
            $trait_methods

            $(for mt in &obj.methods() => $(generate_method(mt, type_helper)))
        }

        class $ffi_converter_name {
            static $cls_name lift(Pointer<Void> ptr) {
                return $cls_name.lift(ptr);
            }

            static Pointer<Void> lower($cls_name value) {
                return value.uniffiClonePointer();
            }

            // A Rust pointer is 8 bytes
            static int allocationSize($cls_name value) {
                return 8;
            }

            static LiftRetVal<$cls_name> read(Uint8List buf) {
                final handle = buf.buffer.asByteData(buf.offsetInBytes).getInt64(0);
                final pointer = Pointer<Void>.fromAddress(handle);
                return LiftRetVal($cls_name.lift(pointer), 8);
            }

            static int write($cls_name value, Uint8List buf) {
                final handle = lower(value);
                buf.buffer.asByteData(buf.offsetInBytes).setInt64(0, handle.address);
                return 8;
            }
        }

        $error_handler_class

        $(stream_glue)
    }
}

fn generate_callback_trait_rust_impl(
    obj: &Object,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let cls_name = &DartCodeOracle::class_name(obj.name());
    let impl_name = format!("_{cls_name}Impl");
    let finalizer_field = format!("_{cls_name}ImplFinalizer");
    let ffi_object_free_name = obj.ffi_object_free().name();
    let ffi_object_clone_name = obj.ffi_object_clone().name();
    let concrete_methods = obj
        .methods()
        .into_iter()
        .map(|method| generate_callback_trait_rust_method(method, type_helper));

    let (finalizer_externals, finalizer) = object_finalizer(obj, type_helper);

    quote! {
        $finalizer_externals

        final class $(&impl_name) implements $cls_name, Finalizable {
            $(&impl_name)._internal(this._ptr) {
                $(&finalizer_field).attach(this, _ptr, detach: this);
            }

            static final UniffiObjectFinalizer $(&finalizer_field) = $finalizer;

            final Pointer<Void> _ptr;
            bool _uniffiDisposed = false;

            Pointer<Void> uniffiClonePointer() {
                if (_uniffiDisposed) {
                    throw StateError($(quoted(format!("Cannot use a disposed {cls_name}"))));
                }
                final pointer = rustCall((status) => $ffi_object_clone_name(_ptr, status));
                UniffiArgumentScope.current?.own((status) => $ffi_object_free_name(pointer, status));
                return pointer;
            }

            void dispose() {
                if (_uniffiDisposed) {
                    return;
                }
                // Reserve call-status storage before committing the closed
                // state, so an allocation failure leaves disposal retryable.
                final status = calloc<RustCallStatus>();
                // Mark closed before calling Rust, including reentrant disposal.
                _uniffiDisposed = true;
                $(&finalizer_field).detach(this);
                try {
                    $ffi_object_free_name(_ptr, status);
                    checkCallStatus(NullRustCallStatusErrorHandler(), status);
                } finally {
                    calloc.free(status);
                }
            }

            $(for method in concrete_methods => $method)
        }
    }
}

fn generate_callback_trait_rust_method(
    method: &Method,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let call_status = &DartCodeOracle::call_status_name(&method.arguments());
    let method_name = DartCodeOracle::fn_name(method.name());
    let dart_args = method
        .arguments()
        .iter()
        .map(|arg| {
            let arg_type = arg.as_renderable().render_type(&arg.as_type(), type_helper);
            let arg_name = DartCodeOracle::var_name(arg.name());
            quote!($arg_type $arg_name)
        })
        .collect::<Vec<_>>();

    let (ret, lifter) = if let Some(ret) = method.return_type() {
        (ret.as_renderable().render_type(ret, type_helper), quote!($(ret.as_codetype().lift())))
    } else {
        (quote!(void), quote!((_) {}))
    };

    let error_handler = if let Some(error_type) = method.throws_type() {
        let error_name = DartCodeOracle::class_name(error_type.name().unwrap_or("UnknownError"));
        let handler_name = format!("{}ErrorHandler", error_name.to_lower_camel_case());
        quote!($(handler_name))
    } else {
        quote!(null)
    };

    let lowered: Vec<_> =
        method.arguments().iter().map(|arg| lower_argument(arg, type_helper)).collect();
    let call = DartCodeOracle::scoped_ffi_call(
        method.ffi_func().name(),
        &method.arguments(),
        true,
        if method.is_async() { None } else { Some(quote!($call_status)) },
        Some(&lowered),
        type_helper,
    );

    if method.is_async() {
        // UniFFI 0.29: async object results complete as `Pointer<Void>`, so lift directly.
        let async_lifter = lifter.clone();

        quote! {
            @override
            Future<$ret> $method_name($(for a in &dart_args => $a,)) {
                return uniffiRustCallAsync(
                    () => $(&call),
                    $(DartCodeOracle::async_poll(method, type_helper.get_ci())),
                    $(DartCodeOracle::async_complete(method, type_helper.get_ci())),
                    $(DartCodeOracle::async_free(method, type_helper.get_ci())),
                    $async_lifter,
                    $error_handler,
                    $(DartCodeOracle::async_finalizer(method, type_helper)),
                );
            }
        }
    } else if ret == quote!(void) {
        quote! {
            @override
            $ret $method_name($(for a in &dart_args => $a,)) {
                return rustCall(($call_status) {
                    $(&call);
                }, $error_handler);
            }
        }
    } else {
        quote! {
            @override
            $ret $method_name($(for a in &dart_args => $a,)) {
                return rustCallWithLifter(
                    ($call_status) => $(&call),
                    $lifter,
                    $error_handler
                );
            }
        }
    }
}

#[allow(unused_variables)]
pub fn generate_method(func: &Method, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    let call_status = &DartCodeOracle::call_status_name(&func.arguments());
    // if func.takes_self_by_arc() {} // TODO: Do something about this condition
    let args = if func.arguments().is_empty() {
        quote!()
    } else {
        quote!({$(for arg in &func.arguments() =>
            $(render_argument_param(
                arg,
                arg.as_renderable().render_type(&arg.as_type(), type_helper),
                type_helper,
            )),
        )})
    };

    let (ret, lifter) = if let Some(ret) = func.return_type() {
        (ret.as_renderable().render_type(ret, type_helper), quote!($(ret.as_codetype().lift())))
    } else {
        (quote!(void), quote!((_) {}))
    };

    // Check if function can throw errors
    let error_handler = if let Some(error_type) = func.throws_type() {
        let error_name = DartCodeOracle::class_name(error_type.name().unwrap_or("UnknownError"));
        // Use the consistent Exception naming for error handlers
        let handler_name = format!("{}ErrorHandler", error_name.to_lower_camel_case());
        quote!($(handler_name))
    } else {
        quote!(null)
    };

    let lowered: Vec<_> =
        func.arguments().iter().map(|arg| lower_argument(arg, type_helper)).collect();
    let call = DartCodeOracle::scoped_ffi_call(
        func.ffi_func().name(),
        &func.arguments(),
        true,
        if func.is_async() { None } else { Some(quote!($call_status)) },
        Some(&lowered),
        type_helper,
    );

    if func.is_async() {
        // UniFFI 0.29: async object results complete as `Pointer<Void>`, so lift directly.
        let async_lifter = lifter.clone();

        quote!(
            Future<$ret> $(DartCodeOracle::fn_name(func.name()))($args) {
                return uniffiRustCallAsync(
                  () => $(&call),
                  $(DartCodeOracle::async_poll(func, type_helper.get_ci())),
                  $(DartCodeOracle::async_complete(func, type_helper.get_ci())),
                  $(DartCodeOracle::async_free(func, type_helper.get_ci())),
                  $async_lifter,
                  $error_handler,
                  $(DartCodeOracle::async_finalizer(func, type_helper)),
                );
            }

        )
    } else if ret == quote!(void) {
        quote!(
            $ret $(DartCodeOracle::fn_name(func.name()))($args) {
                return rustCall(($call_status) {
                    $(&call);
                }, $error_handler);
            }
        )
    } else {
        quote!(
            $ret $(DartCodeOracle::fn_name(func.name()))($args) {
                return rustCallWithLifter(
                    ($call_status) => $(&call),
                    $lifter,
                    $error_handler
                );
            }
        )
    }
}

fn generate_trait_helpers(obj: &Object, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    let mut tokens = quote!();
    let mut generated_display = false;
    let mut generated_debug = false;
    let mut generated_eq = false;
    let mut generated_hash = false;

    for trait_impl in obj.uniffi_traits() {
        match trait_impl {
            UniffiTrait::Display { fmt } => {
                if generated_display {
                    continue;
                }
                let call = trait_method_call(fmt, type_helper, &[]);
                tokens.append(quote! {
                    @override
                    String toString() {
                        return $call;
                    }
                });
                generated_display = true;
            }
            UniffiTrait::Debug { fmt } => {
                if generated_debug {
                    continue;
                }
                let call = trait_method_call(fmt, type_helper, &[]);
                tokens.append(quote! {
                    String debugString() {
                        return $call;
                    }
                });
                generated_debug = true;
            }
            UniffiTrait::Eq { eq, .. } => {
                if generated_eq {
                    continue;
                }
                let call = trait_method_call(eq, type_helper, &[quote!(other)]);
                tokens.append(quote! {
                    @override
                    bool operator ==(Object other) {
                        if (identical(this, other)) {
                            return true;
                        }
                        if (other is! $(DartCodeOracle::class_name(obj.name()))) {
                            return false;
                        }
                        return $call;
                    }
                });
                generated_eq = true;
            }
            UniffiTrait::Hash { hash } => {
                if generated_hash {
                    continue;
                }
                let call = trait_method_call(hash, type_helper, &[]);
                tokens.append(quote! {
                    @override
                    int get hashCode {
                        // Rust Hash returns u64. Dart requires an int hashCode;
                        // BigInt.hashCode preserves equality without narrowing.
                        return ($call).hashCode;
                    }
                });
                generated_hash = true;
            }
        }
    }

    tokens
}

fn trait_method_call(
    method: &Method,
    type_helper: &dyn TypeHelperRenderer,
    arg_exprs: &[dart::Tokens],
) -> dart::Tokens {
    assert_eq!(method.arguments().len(), arg_exprs.len());

    let ffi_name = method.ffi_func().name();

    let error_handler = if let Some(error_type) = method.throws_type() {
        let error_name = DartCodeOracle::class_name(error_type.name().unwrap_or("UnknownError"));
        let handler_name = format!("{}ErrorHandler", error_name.to_lower_camel_case());
        quote!($(handler_name))
    } else {
        quote!(null)
    };

    let mut lowered_args = Vec::new();
    for (arg, expr) in method.arguments().into_iter().zip(arg_exprs.iter()) {
        type_helper.include_once_check(&arg.as_codetype().canonical_name(), &arg.as_type());
        lowered_args.push(DartCodeOracle::type_lower_fn(&arg.as_type(), expr.clone(), type_helper));
    }

    let call = DartCodeOracle::scoped_ffi_call(
        ffi_name,
        &method.arguments(),
        true,
        Some(quote!(status)),
        Some(&lowered_args),
        type_helper,
    );

    if let Some(ret) = method.return_type() {
        type_helper.include_once_check(&ret.as_codetype().canonical_name(), ret);
        let lifter = quote!($(ret.as_codetype().lift()));
        quote!(
            rustCallWithLifter(
                (status) => $(&call),
                $lifter,
                $error_handler
            )
        )
    } else {
        quote!(
            rustCall((status) {
                $(&call);
            }, $error_handler)
        )
    }
}

fn generate_trait_object(obj: &Object, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    type_helper.include_once_check(obj.name(), &obj.as_type());

    let cls_name = &DartCodeOracle::class_name(obj.name());
    let ffi_converter_name = &obj.as_codetype().ffi_converter_name();
    let impl_name = format!("_{cls_name}Impl");
    let finalizer_field = format!("_{cls_name}ImplFinalizer");

    let ffi_object_free_name = obj.ffi_object_free().name();
    let ffi_object_clone_name = obj.ffi_object_clone().name();
    let (finalizer_externals, finalizer) = object_finalizer(obj, type_helper);

    let abstract_methods =
        obj.methods().into_iter().map(|method| generate_interface_method(method, type_helper));

    let concrete_methods =
        obj.methods().into_iter().map(|method| generate_method(method, type_helper));

    quote! {
        abstract class $cls_name {
            factory $cls_name.lift(Pointer<Void> ptr) {
                // UniFFI 0.30.0: Check if handle is from foreign side (lowest bit set)
                final handle = ptr.address;
                final isForeign = (handle & 0x1) != 0;

                if (isForeign) {
                    // Foreign (Dart-side) trait implementations not yet supported
                    throw UnsupportedError("Foreign trait implementations are not yet supported in uniffi-dart");
                }

                // Rust-generated handle (lowest bit is 0)
                return $(&impl_name)._internal(ptr);
            }

            void dispose();

            $(for method in abstract_methods => $method)
        }

        class $ffi_converter_name {
            static $cls_name lift(Pointer<Void> ptr) {
                return $cls_name.lift(ptr);
            }

            static Pointer<Void> lower($cls_name value) {
                if (value is $(&impl_name)) {
                    return value.uniffiClonePointer();
                }
                throw UnsupportedError("Only Rust-implemented $cls_name values are supported.");
            }

            static int allocationSize($cls_name value) {
                if (value is $(&impl_name)) {
                    return $(&impl_name).allocationSize(value);
                }
                throw UnsupportedError("Only Rust-implemented $cls_name values are supported.");
            }

            static LiftRetVal<$cls_name> read(Uint8List buf) {
                final handle = buf.buffer.asByteData(buf.offsetInBytes).getInt64(0);
                final pointer = Pointer<Void>.fromAddress(handle);
                return LiftRetVal($cls_name.lift(pointer), 8);
            }

            static int write($cls_name value, Uint8List buf) {
                final handle = lower(value);
                buf.buffer.asByteData(buf.offsetInBytes).setInt64(0, handle.address);
                return 8;
            }
        }

        $finalizer_externals

        final class $(&impl_name) implements $cls_name, Finalizable {
            $(&impl_name)._internal(this._ptr) {
                $(&finalizer_field).attach(this, _ptr, detach: this);
            }

            static final UniffiObjectFinalizer $(&finalizer_field) = $finalizer;

            final Pointer<Void> _ptr;
            bool _uniffiDisposed = false;

            static int allocationSize($(&impl_name) _) => 8;

            Pointer<Void> uniffiClonePointer() {
                if (_uniffiDisposed) {
                    throw StateError($(quoted(format!("Cannot use a disposed {cls_name}"))));
                }
                final pointer = rustCall((status) => $ffi_object_clone_name(_ptr, status));
                UniffiArgumentScope.current?.own((status) => $ffi_object_free_name(pointer, status));
                return pointer;
            }

            @override
            void dispose() {
                if (_uniffiDisposed) {
                    return;
                }
                // Reserve call-status storage before committing the closed
                // state, so an allocation failure leaves disposal retryable.
                final status = calloc<RustCallStatus>();
                // Mark closed before calling Rust, including reentrant disposal.
                _uniffiDisposed = true;
                $(&finalizer_field).detach(this);
                try {
                    $ffi_object_free_name(_ptr, status);
                    checkCallStatus(NullRustCallStatusErrorHandler(), status);
                } finally {
                    calloc.free(status);
                }
            }

            $(for method in concrete_methods => $method)
        }
    }
}

/// The object type's finalizer, and the relay functions it needs, to declare
/// at the top level. It frees through the type's UniFFI free: from the relay's
/// free thread when the relay is bundled, so it also runs after the object's
/// isolate has exited, and otherwise on the isolate's event loop.
fn object_finalizer(
    obj: &Object,
    type_helper: &dyn TypeHelperRenderer,
) -> (dart::Tokens, dart::Tokens) {
    let free = obj.ffi_object_free().name().to_owned();
    let fallback = quote!((ptr) {
        rustCall((status) => $(&free)(ptr, status));
    });
    // Dispatch mode's companion has no free thread.
    if type_helper.callback_dispatch() {
        return (quote!(), quote!(UniffiObjectFinalizer($fallback, () => null)));
    }
    let ns = type_helper.get_ci().namespace();
    let cls_name = DartCodeOracle::class_name(obj.name());
    let set_free = &format!("_uniffiRelaySetFree{cls_name}");
    let finalize = &format!("_uniffiRelayFinalize{cls_name}");
    let free_type = &quote!(Void Function(Pointer<Void>, Pointer<RustCallStatus>));
    let externals = quote! {
        @Native<Void Function(Pointer<NativeFunction<$free_type>>)>(
            assetId: uniffiCallbackRelayAsset,
            symbol: $(quoted(relay::set_free_symbol(ns, obj.name()))),
        )
        external void $set_free(Pointer<NativeFunction<$free_type>> free);

        @Native<Void Function(Pointer<Void>)>(
            assetId: uniffiCallbackRelayAsset,
            symbol: $(quoted(relay::finalize_symbol(ns, obj.name()))),
        )
        external void $finalize(Pointer<Void> token);
    };
    let finalizer = quote! {
        UniffiObjectFinalizer(
            $fallback,
            () => uniffiRelayFinalizer(
                () => $set_free(Native.addressOf<NativeFunction<$free_type>>($(&free))),
                () => Native.addressOf<NativeFinalizerFunction>($finalize),
            ),
        )
    };
    (externals, finalizer)
}

fn generate_object_interface(
    obj: &Object,
    interface_name: &str,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let method_tokens: Vec<dart::Tokens> = obj
        .methods()
        .into_iter()
        .map(|method| generate_interface_method(method, type_helper))
        .collect();

    if method_tokens.is_empty() {
        quote! {
            abstract class $(interface_name) {}
        }
    } else {
        quote! {
            abstract class $(interface_name) {
                $(for method in method_tokens => $method)
            }
        }
    }
}

fn generate_interface_method(
    method: &Method,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let params = if method.arguments().is_empty() {
        quote!()
    } else {
        quote!({$(for arg in method.arguments() =>
            $(render_argument_param(
                arg,
                arg.as_renderable().render_type(&arg.as_type(), type_helper),
                type_helper,
            )),
        )})
    };
    let ret_type = method_return_type_tokens(method, type_helper);
    let method_name = DartCodeOracle::fn_name(method.name());

    quote!(
        $ret_type $method_name($params);
    )
}

fn method_return_type_tokens(
    method: &Method,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let base = if let Some(ret) = method.return_type() {
        ret.as_renderable().render_type(ret, type_helper)
    } else {
        quote!(void)
    };

    if method.is_async() {
        quote!(Future<$base>)
    } else {
        base
    }
}
