//! Value equality for generated records and enum variants.
//!
//! Records and the variant classes of enums with fields compare by value, as
//! they do in UniFFI's Kotlin, Swift and Python bindings. Lists (including
//! `Uint8List`) and maps compare by their contents, deeply. Everything else
//! uses its own `==`: nested records and enums compare by value, and objects
//! keep identity unless they export `Eq` and `Hash` to the foreign side.

use genco::prelude::*;

/// `operator ==` and `hashCode` for a generated class with the given fields.
///
/// `discriminator` goes into the hash first. Enum variants pass their index,
/// so variants with equal field values don't share a hash.
///
/// The members name no types: Dart infers `bool operator ==(Object other)`
/// and `int get hashCode` from `Object`. A UniFFI type named `Object`, or a
/// field named `bool` or `int`, would otherwise shadow the `dart:core` type.
pub fn value_equality(
    cls_name: &str,
    field_names: &[String],
    discriminator: Option<usize>,
) -> dart::Tokens {
    // A field named `other` would be shadowed by the parameter of `==`.
    let own = |name: &String| {
        if name == "other" {
            format!("this.{name}")
        } else {
            name.clone()
        }
    };
    let equal = std::iter::once(format!("other is {cls_name}"))
        .chain(
            field_names.iter().map(|name| format!("uniffiDeepEquals({}, other.{name})", own(name))),
        )
        .collect::<Vec<_>>()
        .join(" && ");
    let hashed = discriminator
        .map(|d| d.to_string())
        .into_iter()
        .chain(field_names.iter().map(own))
        .collect::<Vec<_>>()
        .join(", ");

    quote! {
        @override
        operator ==(other) => $equal;

        @override
        get hashCode => uniffiDeepHash([$hashed]);
    }
}

/// The deep equality and hashing that [`value_equality`] calls, emitted once
/// into the shared runtime.
pub fn runtime_helpers() -> dart::Tokens {
    quote! {
        // Whether two values of a generated record or enum field are equal.
        // Lists (including `Uint8List`) and maps compare by their contents,
        // deeply, and anything else with its own `==`.
        bool uniffiDeepEquals(Object? a, Object? b) {
            if (identical(a, b)) {
                return true;
            }
            if (a is List && b is List) {
                if (a.length != b.length) {
                    return false;
                }
                for (var i = 0; i < a.length; i++) {
                    if (!uniffiDeepEquals(a[i], b[i])) {
                        return false;
                    }
                }
                return true;
            }
            if (a is Map && b is Map) {
                if (a.length != b.length) {
                    return false;
                }
                // Entries of `b` that no key of `a` finds by lookup. A key whose
                // own `==` is identity (a list, say) can still match one of
                // these by its contents.
                List<MapEntry<Object?, Object?>>? unmatched;
                for (final entry in a.entries) {
                    if (b.containsKey(entry.key)) {
                        if (!uniffiDeepEquals(entry.value, b[entry.key])) {
                            return false;
                        }
                        continue;
                    }
                    unmatched ??= b.entries.where((e) => !a.containsKey(e.key)).toList();
                    final match = unmatched.indexWhere((e) =>
                        uniffiDeepEquals(entry.key, e.key) && uniffiDeepEquals(entry.value, e.value));
                    if (match < 0) {
                        return false;
                    }
                    unmatched.removeAt(match);
                }
                return true;
            }
            return a == b;
        }

        // A hash code consistent with `uniffiDeepEquals`.
        int uniffiDeepHash(Object? value) {
            if (value is List) {
                return Object.hashAll(value.map(uniffiDeepHash));
            }
            if (value is Map) {
                return Object.hashAllUnordered(value.entries.map((e) =>
                    Object.hash(uniffiDeepHash(e.key), uniffiDeepHash(e.value))));
            }
            return value.hashCode;
        }
    }
}
