use genco::prelude::*;
use uniffi_bindgen::interface::{Argument, AsType, DefaultValue, Type};

use super::oracle::DartCodeOracle;
use super::primitives::render_interface_default_value;
use super::render::TypeHelperRenderer;

fn enum_name_from_type(ty: &Type) -> Option<String> {
    match ty {
        Type::Enum { name, .. } => Some(DartCodeOracle::class_name(name)),
        Type::Optional { inner_type } => enum_name_from_type(inner_type),
        _ => None,
    }
}

pub(crate) fn render_default_value(default_value: &DefaultValue, ty: &Type) -> Option<String> {
    render_interface_default_value(default_value, ty, |ty, variant| {
        let enum_name = enum_name_from_type(ty)?;
        let variant_name = DartCodeOracle::enum_variant_name(variant);
        Some(format!("{enum_name}.{variant_name}"))
    })
}

/// The default of a configured custom type, as its lifted Dart expression.
///
/// Dart parameter defaults must be constant, and the configured `lift` of the
/// builtin literal usually isn't, for example `BigInt.parse('100')`. So such a
/// parameter is nullable with no default, and the lifted value is used when it
/// is null.
pub(crate) fn configured_custom_default(
    default_value: &DefaultValue,
    ty: &Type,
    type_helper: &dyn TypeHelperRenderer,
) -> Option<String> {
    let Type::Custom { name, builtin, .. } = ty else {
        return None;
    };
    let config = type_helper.custom_type_config(name)?;
    let literal = render_default_value(default_value, builtin)?;
    Some(config.lift(&literal))
}

fn argument_custom_default(arg: &Argument, type_helper: &dyn TypeHelperRenderer) -> Option<String> {
    configured_custom_default(arg.default_value()?, &arg.as_type(), type_helper)
}

/// The FFI lowering of an argument, applying a configured custom type's
/// default when the argument is null.
pub(crate) fn lower_argument(arg: &Argument, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    match argument_custom_default(arg, type_helper) {
        Some(default_expr) => {
            let name = DartCodeOracle::var_name(arg.name());
            DartCodeOracle::type_lower_fn(&arg.as_type(), quote!(($name ?? $default_expr)))
        }
        None => DartCodeOracle::lower_arg_with_callback_handling(arg),
    }
}

pub(crate) fn render_argument_param(
    arg: &Argument,
    type_tokens: dart::Tokens,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    let name = DartCodeOracle::var_name(arg.name());

    if argument_custom_default(arg, type_helper).is_some() {
        return quote!($type_tokens? $name);
    }
    if let Some(default_value) = arg.default_value() {
        if let Some(default_expr) = render_default_value(default_value, &arg.as_type()) {
            quote!($type_tokens $name = $(default_expr))
        } else {
            quote!(required $type_tokens $name)
        }
    } else {
        quote!(required $type_tokens $name)
    }
}
