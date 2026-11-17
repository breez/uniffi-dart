//! Public input types differ from stored/lifted types only at u64 leaves.
//!
//! A custom type configured with `custom_types` is its own Dart type, lowered
//! by its own converter, so these helpers stop there. An unconfigured custom
//! type is a typedef of its builtin and takes the builtin's input.
use genco::prelude::*;
use uniffi_bindgen::interface::Type;

use super::oracle::DartCodeOracle;
use super::render::TypeHelperRenderer;

pub(crate) fn accepts_u64(ty: &Type, type_helper: &dyn TypeHelperRenderer) -> bool {
    match ty {
        Type::UInt64 => true,
        Type::Optional { inner_type } | Type::Sequence { inner_type } => {
            accepts_u64(inner_type, type_helper)
        }
        Type::Map { key_type, value_type } => {
            accepts_u64(key_type, type_helper) || accepts_u64(value_type, type_helper)
        }
        Type::Custom { name, builtin, .. } => {
            type_helper.custom_type_config(name).is_none() && accepts_u64(builtin, type_helper)
        }
        // Records and enum variants normalize their fields in their constructors.
        _ => false,
    }
}

pub(crate) fn input_type(ty: &Type, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
    match ty {
        // A component can itself define a Rust object named Object.
        Type::UInt64 => quote!(uniffiCore.Object),
        Type::Optional { inner_type } => quote!($(input_type(inner_type, type_helper))?),
        Type::Sequence { inner_type } => quote!(List<$(input_type(inner_type, type_helper))>),
        Type::Map { key_type, value_type } => {
            let key = input_type(key_type, type_helper);
            let value = input_type(value_type, type_helper);
            quote!(Map<$key, $value>)
        }
        Type::Custom { builtin, .. } if accepts_u64(ty, type_helper) => {
            input_type(builtin, type_helper)
        }
        _ => DartCodeOracle::dart_type_label(Some(ty)),
    }
}

pub(crate) fn normalize(
    ty: &Type,
    value: dart::Tokens,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    if !accepts_u64(ty, type_helper) {
        return value;
    }
    match ty {
        Type::UInt64 => quote!(FfiConverterUInt64.normalize($value)),
        Type::Optional { inner_type } => {
            let inner = normalize(inner_type, value.clone(), type_helper);
            quote!(($value == null ? null : $inner))
        }
        Type::Sequence { inner_type } => {
            let inner = normalize(inner_type, quote!(element), type_helper);
            let output = DartCodeOracle::dart_type_label(Some(inner_type));
            quote!($value.map<$output>((element) => $inner).toList())
        }
        Type::Map { key_type, value_type } => {
            let key = normalize(key_type, quote!(key), type_helper);
            let val = normalize(value_type, quote!(value), type_helper);
            quote!(uniffiNormalizeU64Map($value, (key) => $key, (value) => $val))
        }
        Type::Custom { builtin, .. } => normalize(builtin, value, type_helper),
        _ => unreachable!(),
    }
}

/// `lower` and `write` validate a bare u64 themselves, so they stay safe entry
/// points for nested converters. Only containers need the walk first, because
/// their converters are typed on the stored `BigInt` form.
pub(crate) fn normalize_for_lower(
    ty: &Type,
    value: dart::Tokens,
    type_helper: &dyn TypeHelperRenderer,
) -> dart::Tokens {
    if matches!(ty, Type::UInt64) {
        value
    } else {
        normalize(ty, value, type_helper)
    }
}

/// How a record or enum-variant constructor accepts one field. A field that
/// takes flexible u64 input cannot be an initializing formal (`this.name`),
/// since the parameter is wider than the stored field: it becomes a plain
/// parameter plus an initializer that stores the normalized value. The caller
/// decides `required` and defaults once, for both kinds of field.
pub(crate) fn constructor_field(
    ty: &Type,
    name: &str,
    initializing_formal: dart::Tokens,
    type_helper: &dyn TypeHelperRenderer,
) -> (dart::Tokens, Option<dart::Tokens>) {
    if !accepts_u64(ty, type_helper) {
        return (initializing_formal, None);
    }
    let value = normalize(ty, quote!($name), type_helper);
    (quote!($(input_type(ty, type_helper)) $name), Some(quote!($name = $value)))
}

/// Render collected initializers as `: a = ..., b = ...`, or nothing.
pub(crate) fn initializer_list(initializers: &[dart::Tokens]) -> dart::Tokens {
    if initializers.is_empty() {
        return quote!();
    }
    let mut values = dart::Tokens::new();
    for (i, init) in initializers.iter().enumerate() {
        if i != 0 {
            values.append(quote!(,));
        }
        values.append(init.clone());
    }
    quote!(: $values)
}
