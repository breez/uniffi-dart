<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Free Rust objects from a NativeFinalizer when the companion is bundled

Stacked on #PR2 (the callback relay). Review that first: this PR is the commit
after it.

Generated objects free their Rust side from a `dart:core` `Finalizer`. Its
callback runs on the object's isolate, on a later turn of its event loop, so it
never runs once that isolate has exited. After a Flutter hot restart, or an
`Isolate.run` that created objects, every Rust object the old isolate held stays
alive for the life of the process. For an SDK that's the whole instance: its
connections, database handles and background tasks keep running beside the new
isolate's instance.

A `NativeFinalizer` doesn't have that problem. The VM runs it when the object is
collected, including after its isolate has exited, and when the isolate group
shuts down, which is what a hot restart does. It needs a native function that
takes one pointer, and UniFFI's free functions also take a call status, so
#PR2's companion now provides one per object type. When the companion is
bundled, each object type registers its UniFFI free with it, and objects are
attached to a `NativeFinalizer` that calls the companion. Without the
companion, objects keep the `Finalizer`.

### The free thread

The finalizer callback doesn't free the object itself. It runs during garbage
collection or group shutdown, on any thread, and the `NativeFinalizer` docs say
it must not call into the VM, apart from deleting persistent handles. A Rust
`Drop` can, for example by releasing a Dart callback object, whose free the
relay posts to its isolate with `Dart_PostCObject`. So the callback only queues
the object, and a thread the companion owns frees it, as Kotlin's `Cleaner`
thread does. UniFFI objects already have to tolerate being freed on another
thread, since Kotlin frees them on the Cleaner thread and Swift's `deinit` runs
wherever the last reference goes.

### Other changes

- Generation now writes the companion for components with objects as well as
  callbacks, so packages without callbacks get the fix too. Nothing changes
  for them unless they bundle it. If you'd rather keep the companion to
  callbacks, it's a one-line gate.
- Generated object classes, and the Rust-backed wrappers of trait interfaces,
  implement `Finalizable`, which a `NativeFinalizer` requires. That also keeps
  the compiler from letting an object die while one of its methods still uses
  it.
- The ABI hash covers each object type's free, so adding or removing an object
  type means rebuilding the companion, as callbacks already do.

### Limits

- A free still queued when the process exits may not run. The OS reclaims the
  memory anyway.
- After a hot restart, the old isolate's objects are freed on the companion's
  thread while the new isolate starts. An object whose `Drop` blocks only
  delays the frees queued behind it, but one that tears down shared state, like
  a database another instance is opening, has to tolerate the overlap.
- Freeing an object only drops the references Dart held. Rust work that holds
  its own references, such as background tasks, keeps running unless the
  object's `Drop` stops it.
- `callback_dispatch` keeps the `Finalizer`, since its companion has no free
  thread.

### Testing

`callback-relay` adds an object whose Rust `Drop` counts drops. One made
unreachable in the test's isolate is freed with or without the companion. One
made in `Isolate.run` is freed after that isolate exits, with collection forced
by allocating. Before this change it was never freed: the count didn't move in
30 seconds. Every other object fixture now also runs through the
`NativeFinalizer`, since the harness bundles the companion whenever generation
writes one, which covers `dispose()` detaching it.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 46
test targets, 321 Dart cases. `cargo clippy --workspace --all-targets -D
warnings` and `cargo fmt --check` are clean. The generated companions of the
`callback-relay`, `dart_async` and `hello_world` fixtures are clippy-clean with
`-D warnings` on Rust 1.85.0 and on stable, and #186's generated probe passes
in JIT and AOT.
