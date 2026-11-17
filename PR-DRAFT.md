<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Deliver async, fire-and-forget and free callbacks through listeners

Rust code calls Dart callbacks from wherever it happens to run: a tokio worker,
a `std::thread`, a thread owned by a C library. The bindings create every
callback with `Pointer.fromFunction`, which aborts the VM with `Cannot invoke
native callback outside an isolate` when it's called from any thread other than
its isolate's (#161). Event listeners and loggers are the common case. Rust
calls them from background tasks and doesn't need anything back.

Callbacks that hand nothing back synchronously now go through
`NativeCallable.listener`, which accepts calls from any thread: async methods,
sync methods with no return value and no error type, and `uniffi_free`. A
listener call runs on a later turn of the owning isolate's event loop, with
`keepIsolateAlive = false`. Sync methods that return a value or can throw, and
`uniffi_clone`, still use `Pointer.fromFunction`, because Rust reads their
result as soon as the call returns. A follow-up PR relays those.

This is on by default. A listener-delivered callback now runs after the Rust
call that made it has returned, even on the isolate's own thread, so this is a
breaking change for code that observed those effects synchronously.

### Design

An async method no longer writes Rust's `UniffiForeignFuture` out-param. A
listener runs after Rust's call has returned, when that pointer is gone, and
Rust pre-fills it with a no-op free callback, so Rust is fine without it. That
removes the cancel hook. Dart futures can't be cancelled anyway, so the hook
only let a late completion be skipped. Now the Dart work runs to completion and
Rust discards a result that arrives after it stopped waiting. The `cancelled`
state and its handle map are gone from the runtime.

The VM delivers an isolate's listener calls in the order they were made. The
shared runtime adds `uniffiCallbackBarrier()`, which posts a marker through a
listener and completes when it runs, so tests and apps can wait for callbacks
that were already queued. Generation rejects an export that would also be named
`uniffiCallbackBarrier`.

UniFFI overwrites a callback type's vtable each time one is installed, so the
last isolate to lower a callback of a given type owns that type for the whole
process. The second commit makes handle maps start from a random per-isolate
prefix in the upper 32 bits. After a Flutter hot restart, handles Rust kept
from the old isolate then miss in the new isolate's maps instead of reaching,
or freeing, the object that got the same number. It's separable if you'd
rather take it on its own.

`docs/callback-threads.md` covers delivery, ordering, cancellation, isolate
ownership, `keepIsolateAlive` and hot restart.

### Relation to #186

With `callback_dispatch` on, callback generation is unchanged. Only the shared
runtime additions (the barrier and the handle prefix) reach it. Its companion
already relays every synchronous callback, and it rejects async ones, so it
still uses `Pointer.fromFunction` everywhere. The generated probe passes
unchanged. The follow-up PR reuses #186's companion core for the sync
methods that return a value or throw, without the worker threads, so it
applies to async APIs, objects and external types too. `callback_dispatch`
stays the option for a synchronous Rust call that blocks on a worker thread
calling back synchronously.

### Limits

- Calling a sync method that returns a value or can throw, or `uniffi_clone`,
  from another thread still aborts. The follow-up PR fixes that.
- Calls on the isolate's own thread can now run out of order. A
  fire-and-forget method is queued, but a value-returning method on the same
  object still runs inline. If Rust calls `log()` and then `logCount()` in one
  call, `logCount()` runs first. The `callback-threads` fixture pins this down.
  With the follow-up PR's companion bundled, every callback made on the
  isolate's thread runs inline again, so the order comes back.
- If the owning isolate exits while Rust still holds its callbacks, the next
  call aborts the process with `Callback invoked after it has been deleted`. I
  measured this on Dart 3.14 with a listener whose isolate had exited, with
  `keepIsolateAlive` both true and false. So `Isolate.run`, `compute`, and the
  window after a hot restart before the new isolate re-registers are
  unsupported while Rust retains callbacks. The docs say so.
- `keepIsolateAlive = false` means a command line program exits when `main`
  finishes, even if Rust holds a logger. Keeping the isolate alive while
  callbacks exist would make any program that installs a long-lived logger
  never exit. Programs that expect a callback should wait for it.
- A synchronous Rust call made from Dart that blocks until a listener-delivered
  callback has run deadlocks, because the isolate's thread is blocked in Rust.
  That was already true of async callbacks.
- The handle prefix is 64-bit only. Until callback handles are lowered as
  `Uint64` (#190), 32-bit targets carry them in a `Pointer`, which would drop
  the prefix.

### Testing

The new `callback-threads` fixture calls a callback interface and a foreign
trait from std threads and tokio tasks. Its cases run in a child process so a
VM abort fails the test instead of the runner. They cover async methods, 32
concurrent calls, fire-and-forget calls, frees on worker threads, a Rust future
dropped before Dart completes, foreign-trait errors, barrier ordering over 200
calls from one thread, a value method overtaking an earlier fire-and-forget
call, cross-isolate handle prefixes, and a program exiting while Rust holds a
callback. With fire-and-forget methods and frees switched
back to `Pointer.fromFunction`, the child aborts with the #161 error and both
fixture tests fail.

Tests that checked fire-and-forget effects synchronously now await the
barrier: `proc-macro`'s `doNothing` and the `callbacks` reentrant disposal
case. The `dart_async` completion tests check that the out-param is left
untouched, including for a late success or error.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 38 test
targets, 285 Dart cases.
`cargo clippy --workspace --all-targets -D warnings` and `cargo fmt --check`
are clean. The generated dispatch probe passes in JIT and AOT, with the
companion built on Rust 1.85.0 (1.85.1 wasn't installed). I also ran the
fixtures on Android (an arm64 emulator and a 32-bit Galaxy A13) from a UniFFI
0.29 port of the opt-in version of this change, but not this branch.

Related to #161. A follow-up PR handles the remaining sync callbacks.
