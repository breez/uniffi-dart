<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Deliver async callback methods and frees through listeners

Rust code calls Dart callbacks from wherever it happens to run: a tokio worker,
a `std::thread`, a thread owned by a C library. The bindings create every
callback with `Pointer.fromFunction`, which aborts the VM with `Cannot invoke
native callback outside an isolate` when it's called from any thread other than
its isolate's (#161). Async callbacks are the common case, for example event
listeners and storage traits, which Rust calls from its async runtime. Frees are
the other: Rust drops a callback object on whatever thread held it last.

Async methods and `uniffi_free` now go through `NativeCallable.listener`, which
accepts calls from any thread and runs them on a later turn of the owning
isolate's event loop, with `keepIsolateAlive = false`. Neither changes what a
caller can observe on the isolate's own thread, apart from timing. Rust only
sees an async method's result by awaiting it, and a free only releases a handle
Rust no longer uses. Sync methods keep `Pointer.fromFunction`, so they still run
inline and in order on the isolate's thread. A follow-up PR makes them callable
from any thread through #186's companion.

### Design

An async method no longer writes Rust's `UniffiForeignFuture` out-param. A
listener runs after Rust's call has returned, when that pointer is gone, and
Rust pre-fills it with a no-op free callback, so Rust is fine without it. That
removes the cancel hook. Dart futures can't be cancelled anyway, so the hook
only let a late completion be skipped. Now the Dart work runs to completion and
Rust discards a result that arrives after it stopped waiting. The `cancelled`
state and its handle map are gone from the runtime.

Listener calls run later even on the isolate's own thread, so handle counts
change slightly after the Rust call returns. The VM delivers an isolate's
listener calls in the order they were made, so the shared runtime adds
`uniffiCallbackBarrier()`. It posts a marker through a listener and completes
when the marker runs. Generation rejects an export that would also be named
`uniffiCallbackBarrier`.

UniFFI overwrites a callback type's vtable each time one is installed, so the
last isolate to lower a callback of a given type owns that type for the whole
process. The second commit makes handle maps start from a random per-isolate
prefix in the upper 32 bits. After a Flutter hot restart, handles Rust kept
from the old isolate then miss in the new isolate's maps instead of reaching,
or freeing, the object that got the same number. It's separable if you'd
rather take it on its own.

The last commit deals with what the prefix exposes. A call through an old
handle reported a stale-handle error, which UniFFI turns into a Rust panic when
the method has no error type. In an app built with `panic = "abort"`, a hot
restart then ended the process at the first log line or event after the
restart. For a method that returns nothing and can't throw, the call is now
dropped: Rust sees it succeed, and the bindings print one warning per isolate.
Calls that return a value or can throw still fail, since Rust needs the value
or can handle the error.

`docs/callback-threads.md` covers delivery, cancellation, isolate ownership,
`keepIsolateAlive` and hot restart.

### Relation to #186

With `callback_dispatch` on, callback generation is unchanged. Only the shared
runtime additions (the barrier and the handle prefix) reach it. Its companion
already relays the free with every other synchronous callback, and it rejects
async methods, so it still uses `Pointer.fromFunction` everywhere. The
generated probe passes unchanged. The follow-up PR reuses #186's companion core
for every callback outside dispatch mode, without the worker threads, so it
applies to async APIs, objects and external types too.

### Limits

- Calling a sync method or `uniffi_clone` from another thread still aborts.
  The follow-up PR fixes that.
- If the owning isolate exits while Rust still holds its callbacks, the next
  async call or free aborts the process with `Callback invoked after it has
  been deleted`. I measured this on Dart 3.14 with a listener whose isolate had
  exited, with `keepIsolateAlive` both true and false. So `Isolate.run`,
  `compute`, and the window after a hot restart before the new isolate
  re-registers are unsupported while Rust retains callbacks. Today the same
  calls abort as well, from the wrong thread or isolate. The follow-up PR
  makes them fail instead of aborting when the companion is bundled.
- A call through an old handle to a method that returns a value and has no
  error type still panics in Rust, because there's no value to give it.
- An async Rust function that Dart called and that is still running when its
  isolate is replaced in a hot restart, or killed, wakes the old isolate's
  continuation listener when it finishes, and the process crashes. That's how
  `uniffiRustCallAsync` already behaves. The follow-up PR routes continuations
  through the companion when it's bundled.
- `keepIsolateAlive = false` means a command line program exits when `main`
  finishes, even if Rust holds a callback. Keeping the isolate alive while
  callbacks exist would make any program that installs a long-lived callback
  never exit. Programs that expect a callback should wait for it.
- A synchronous Rust call made from Dart that blocks until an async callback
  completes deadlocks, because the isolate's thread is blocked in Rust. That
  was already true.
- The handle prefix is 64-bit only. Until callback handles are lowered as
  `Uint64` (#190), 32-bit targets carry them in a `Pointer`, which would drop
  the prefix. #190 also touches the `dart_async` completion tests changed here.

### Testing

The new `callback-threads` fixture calls a callback interface and a foreign
trait from std threads and tokio tasks. Its cases run in a child process so a
VM abort fails the test instead of the runner. They cover async methods, 32
concurrent calls, frees on worker threads and on the isolate's thread, a Rust
future dropped before Dart completes, foreign-trait errors, the barrier,
cross-isolate handle prefixes, and a program exiting while Rust holds a
callback. One case keeps a callback from an isolate that has exited and calls
it after the test's isolate has taken the vtable over. Without the last commit,
its sync and async void calls panicked in Rust with `Callback interface
failure: UnexpectedUniFFICallbackError(reason: "UniFfi::UnexpectedStaleHandle")`.

The `dart_async` completion tests now check that the out-param is left
untouched, including for a late success or error.

With async methods and frees switched back to `Pointer.fromFunction`, the child
aborts with the #161 error and both fixture tests fail.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 38 test
targets, 283 Dart cases. `cargo clippy --workspace --all-targets -D warnings`
and `cargo fmt --check` are clean. The generated dispatch probe passes in JIT
and AOT, with the companion built on Rust 1.85.0 (1.85.1 wasn't installed). I
also ran the fixtures on Android (an arm64 emulator and a 32-bit Galaxy A13)
from a UniFFI 0.29 port of an earlier opt-in version of this change, but not
this branch.

Related to #161. A follow-up PR handles the sync callbacks.
