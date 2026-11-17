<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Keep generated callback functions out of the library's top-level names

The generator writes each callback type's internals as top-level Dart names in
the component file:

- the trampoline for each method, `{trait}{Method}`;
- its function pointer, `{trait}{Method}Pointer`;
- `{trait}FreeCallback`, `{trait}CloneCallback` and their pointers;
- the vtable, `{trait}VTable`, and `init{Trait}VTable`.

Exported functions live in the same scope. So an export like `ledger_cash_total`
breaks the bindings when a callback interface `Ledger` has a method
`cash_total`, because both become `ledgerCashTotal`. Two callback types can also
collide with each other: `Ledger.cash_total` and `LedgerCash.total` generate the
same name. The result is generated Dart that doesn't compile, with no
workaround except renaming the Rust API.

These now live as static members of one class per callback type,
`UniffiCallbackInterface{Name}Impl`. #156 fixed object converters the same
way, by moving their statics into a sibling class. UniFFI's own Kotlin and
Swift bindings likewise keep each callback type's internals in a
`uniffiCallbackInterface{Name}` object or struct. Inside the class:

- each method's trampoline is `{method}Callback`, and its pointer `{method}Pointer`;
- free and clone are `uniffiFree` and `uniffiClone`, with `uniffiFreePointer` and `uniffiClonePointer`;
- the vtable is `uniffiVTable`, set up by `uniffiInit()`.

The two suffixes can't produce the same name for different methods, and
neither can name an `Object` member like `toString`. The class name's `Impl`
suffix keeps it apart from the existing `UniffiCallbackInterface{Name}Free`
style typedefs, so a type named `Foo` and one named `FooFree` don't collide
either. `Pointer.fromFunction` accepts static methods, so nothing else changes.

These were never meant as API, but they were public, and a few fixture tests
call them directly to check completion and cleanup paths. Those tests now use
the class, for example
`UniffiCallbackInterfaceAsyncByteSourceImpl.valueCallback(...)`. The CHANGELOG
says so for anyone else who did the same.

### Testing

The new `callback-name-collisions` fixture has exports named like each of the
old internals, plus the two colliding callback types. With `main`'s generator,
its bindings fail `dart analyze` with nine `duplicate_definition` errors,
covering every kind of collision above. With this change it compiles and its tests pass,
including calls through every callback.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 38 test
targets, 271 Dart cases. `cargo clippy --workspace --all-targets -D warnings`
and `cargo fmt --check` are clean. The generated dispatch probe from #186
passes in JIT and AOT, since dispatch installs its vtables through the same
code.
