# Custom types

A UniFFI custom type is a Rust type that crosses the FFI as a builtin, for
example `uniffi::custom_type!(u128, String, { ... })`, which sends a `u128` as
its decimal string.

Without configuration, the bindings call the custom type by its own name and
make it a `typedef` of the builtin, so Dart code sees the builtin:

```dart
typedef U128 = String;
```

## Mapping a custom type to a Dart type

Like UniFFI's own Kotlin and Swift bindings, the Dart bindings can map a custom
type to a Dart type of your choice in `uniffi.toml`:

```toml
[bindings.dart.custom_types.u128]
type_name = "BigInt"
lift = "BigInt.parse({})"
lower = "{}.toString()"
```

Every function, method, record, enum, error and callback then uses `BigInt`,
and the generated converter calls `lift` on the builtin value it receives and
`lower` on the value it sends.

The keys are the ones Kotlin and Swift accept:

| Key | Meaning |
|---|---|
| `type_name` | The Dart type. The custom type's name becomes a `typedef` of it. Without it, the custom type's own name must already be a Dart type, for example one that `imports` brings in. |
| `imports` | Import URIs the generated file needs, such as `dart:math` or `package:decimal/decimal.dart`. |
| `lift` | A Dart expression that turns the builtin value `{}` into the Dart type. |
| `lower` | A Dart expression that turns the Dart value `{}` into the builtin value. |
| `into_custom`, `from_custom` | Older names for `lift` and `lower`. `lift` and `lower` win if both are set. |

Every `{}` in `lift` and `lower` is replaced. A missing expression passes the
value through unchanged.

The table name is the custom type's name as UniFFI knows it, as in the
Kotlin and Swift bindings: `u128` for `uniffi::custom_type!(u128, ...)`, or
`Url` for `uniffi::custom_type!(Url, ...)`. (UniFFI's Python bindings key
their table by the Python class name instead, such as `U128`.)

In library mode, the configuration comes from the `uniffi.toml` of the crate
that defines the custom type.

### Another example, with an import

```toml
[bindings.dart.custom_types.Money]
imports = ["package:my_app/money.dart"]
lift = "Money.fromCents({})"
lower = "{}.cents"
```

Here `Money` is a custom type over `i64`, and the imported library declares a
Dart class named `Money`.

## Defaults

A default value for a configured custom type is the builtin literal passed
through `lift`, which Dart can't use as a parameter default because it isn't
constant. So the parameter is nullable, and the bindings use the lifted
default when it's null:

```dart
// #[uniffi(default = "100")] pub amount: u128
Order({U128? amount}) : amount = amount ?? BigInt.parse('100');
```

## Limits

A synchronous callback method that returns a custom type over a scalar, such as
`i64`, doesn't generate valid Dart yet, configured or not (#162).
