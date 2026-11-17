<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Type u64 callback results as BigInt

Based on #188 (`f0b7a21`).

#188 types u64 results of callback and foreign-trait methods as `Object`
(`Future<Object>` when async), since Dart hands them to Rust and they accept
`int` or `BigInt` like other inputs. That takes away type checking where the
Dart developer writes the body: returning a `String` or the wrong variable
compiles, and only fails when Rust calls it.

These results now use `BigInt` (`Future<BigInt>`, `List<BigInt?>`, a u64 alias
by its own name), matching the callback's parameters and the Rust-backed side
of a `with_foreign` trait. Lowering is unchanged, so a negative or oversized
`BigInt` still fails #188's range check and reaches Rust as a callback error.
Arguments, constructors and record fields keep #188's `int` or `BigInt` input.

The type-limits callbacks return `BigInt` now, with a new case that only
compiles if the results are statically `BigInt`. The out-of-range result cases
stay.
