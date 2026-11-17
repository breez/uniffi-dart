use genco::prelude::*;
use serde::{Deserialize, Serialize};
use uniffi_bindgen::interface::{AsType, FfiType, Type};

use super::oracle::{AsCodeType, DartCodeOracle};
use super::render::{Renderable, TypeHelperRenderer};
use super::CodeType;

/// `[bindings.dart.custom_types.<Name>]`: maps a custom type to a Dart type,
/// with the keys UniFFI's own Kotlin and Swift bindings accept. `<Name>` is the
/// custom type's name as UniFFI knows it, for example `u128` for
/// `uniffi::custom_type!(u128, String)`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomTypeConfig {
    /// Dart import URIs the generated file needs, such as `dart:math` or
    /// `package:decimal/decimal.dart`.
    pub imports: Option<Vec<String>>,
    /// The Dart type to use. Without it, the custom type's own name must
    /// already be a Dart type, for example one that `imports` provides.
    pub type_name: Option<String>,
    /// Older name for `lift`.
    pub into_custom: String,
    /// Expression that turns the builtin value `{}` into the Dart type.
    pub lift: String,
    /// Older name for `lower`.
    pub from_custom: String,
    /// Expression that turns the Dart value `{}` into the builtin value.
    pub lower: String,
}

impl CustomTypeConfig {
    /// `lift` applied to the Dart expression `value`.
    pub fn lift(&self, value: &str) -> String {
        Self::apply(&self.lift, &self.into_custom, value)
    }

    /// `lower` applied to the Dart expression `value`.
    pub fn lower(&self, value: &str) -> String {
        Self::apply(&self.lower, &self.from_custom, value)
    }

    fn apply(expression: &str, alias: &str, value: &str) -> String {
        let expression = if expression.is_empty() { alias } else { expression };
        if expression.is_empty() {
            value.to_owned()
        } else {
            expression.replace("{}", value)
        }
    }
}

#[derive(Debug)]
pub struct CustomCodeType {
    name: String,
    module_path: String,
    builtin: Box<Type>,
}

impl CustomCodeType {
    pub fn new(name: String, module_path: String, builtin: Box<Type>) -> Self {
        CustomCodeType { name, module_path, builtin }
    }
}

impl CodeType for CustomCodeType {
    fn type_label(&self) -> String {
        DartCodeOracle::class_name(&self.name)
    }
}

impl AsType for CustomCodeType {
    fn as_type(&self) -> Type {
        Type::Custom {
            name: self.name.clone(),
            module_path: self.module_path.clone(),
            builtin: self.builtin.clone(),
        }
    }
}

impl Renderable for CustomCodeType {
    fn render_type_helper(&self, type_helper: &dyn TypeHelperRenderer) -> dart::Tokens {
        type_helper.include_once_check(&self.name, &self.as_type());

        let ffi_converter_name = &self.ffi_converter_name();
        let type_name = &self.type_label();
        let builtin_converter = &(*self.builtin).as_codetype().ffi_converter_name();
        let builtin_name = DartCodeOracle::dart_type_label(Some(&*self.builtin));

        let Some(config) = type_helper.custom_type_config(&self.name) else {
            // Without configuration the custom type is its builtin.
            return quote! {
                typedef $(type_name) = $(builtin_name);
                typedef $(ffi_converter_name) = $(builtin_converter);
            };
        };

        // The builtin's converter does the FFI work. The configured
        // expressions convert between its values and the Dart type.
        let ffi_type = DartCodeOracle::ffi_dart_type_label(Some(&FfiType::from(&*self.builtin)));
        let lift_builtin = config.lift("builtinValue");
        let lower_value = config.lower("value");
        quote! {
            $(if let Some(concrete) = &config.type_name {
                typedef $(type_name) = $(concrete);
            })

            class $(ffi_converter_name) {
                static $(type_name) lift($(&ffi_type) value) {
                    final builtinValue = $(builtin_converter).lift(value);
                    return $(&lift_builtin);
                }

                static $(&ffi_type) lower($(type_name) value) {
                    final builtinValue = $(&lower_value);
                    return $(builtin_converter).lower(builtinValue);
                }

                static LiftRetVal<$(type_name)> read(Uint8List buf) {
                    final builtin = $(builtin_converter).read(buf);
                    final builtinValue = builtin.value;
                    return LiftRetVal($(&lift_builtin), builtin.bytesRead);
                }

                static int write($(type_name) value, Uint8List buf) {
                    final builtinValue = $(&lower_value);
                    return $(builtin_converter).write(builtinValue, buf);
                }

                static int allocationSize($(type_name) value) {
                    final builtinValue = $(&lower_value);
                    return $(builtin_converter).allocationSize(builtinValue);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CustomTypeConfig;

    fn config(toml: &str) -> CustomTypeConfig {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn lift_and_lower_take_precedence_over_their_older_names() {
        let both = config(
            "lift = \"BigInt.parse({})\"\ninto_custom = \"ignored({})\"\n\
             lower = \"{}.toString()\"\nfrom_custom = \"ignored({})\"",
        );
        assert_eq!(both.lift("v"), "BigInt.parse(v)");
        assert_eq!(both.lower("v"), "v.toString()");
        let older = config("into_custom = \"Uri.parse({})\"\nfrom_custom = \"{}.toString()\"");
        assert_eq!(older.lift("v"), "Uri.parse(v)");
        assert_eq!(older.lower("v"), "v.toString()");
    }

    #[test]
    fn missing_expressions_pass_the_value_through() {
        let bare = config("type_name = \"Money\"");
        assert_eq!(bare.lift("v"), "v");
        assert_eq!(bare.lower("v"), "v");
    }

    #[test]
    fn every_placeholder_is_replaced() {
        let twice = config("lift = \"Point({}, {})\"");
        assert_eq!(twice.lift("v"), "Point(v, v)");
    }
}
