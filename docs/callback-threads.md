# Callbacks from other threads

Rust may call a Dart callback interface or foreign trait from any thread: a
`std::thread`, a tokio worker or a thread owned by a C library. A callback
created with `Pointer.fromFunction` can only run on the thread of the isolate
that created it. A call from anywhere else aborts the process with
`Cannot invoke native callback outside an isolate` (#161).

The generated bindings deliver every callback that doesn't hand a value back
synchronously through
[`NativeCallable.listener`](https://api.dart.dev/dart-ffi/NativeCallable/NativeCallable.listener.html),
which accepts calls from any thread:

| Callback | Delivery | From any thread |
|---|---|---|
| async method | listener | yes |
| sync method with no return value and no error type | listener | yes |
| `uniffi_free` (Rust dropped the callback object) | listener | yes |
| sync method that returns a value or can throw | `Pointer.fromFunction` | only the isolate's own thread |
| `uniffi_clone` | `Pointer.fromFunction` | only the isolate's own thread |

The last two must write their result before Rust's call returns, which a
listener can't do. Calling them from another thread still aborts.

## When callbacks run

A listener call is queued to the isolate and runs on a later event-loop turn.
This is true even when Rust makes the call on the isolate's own thread, during a
synchronous Rust function. So:

- A Rust function that calls a fire-and-forget method returns before the Dart
  method runs.
- A sync method that returns a value still runs inline, so it can run before
  an earlier fire-and-forget call on the same object. If Rust calls `log()`
  and then `logCount()` during one call on the isolate's thread, `logCount()`
  doesn't see the log yet.
- An exception from a fire-and-forget method can't reach Rust. It becomes an
  uncaught error in the isolate's zone.
- A callback object that Rust drops is removed from the bindings' handle map
  slightly later.

The VM runs an isolate's listener calls in the order they were made. To observe
their effects, for example in a test, await `uniffiCallbackBarrier()`. It
completes after every callback that was queued before the call. For an async
method it waits until the method has started, not until its future has
completed.

```dart
logInOrder(sink: sink, count: 3); // Rust calls sink.log three times.
await uniffiCallbackBarrier();
expect(sink.logs, ['0', '1', '2']);
```

## Async methods and cancellation

Rust passes a completion callback, which may be called from any thread, and
pre-fills the method's `UniffiForeignFuture` out-parameter. The bindings never
write that out-parameter, because the Dart method runs after Rust's call has
returned. If Rust drops the future, for example on a timeout, the Dart method
still runs to completion. Rust ignores the late result.

## Isolates

UniFFI keeps one vtable per callback type for the whole process. An isolate
installs its vtable the first time it passes a callback of that type to Rust,
replacing any earlier one. From then on, Rust calls of that type reach that
isolate, including calls for objects registered by other isolates. On 64-bit
targets those fail with a stale-handle error. On 32-bit targets they can reach
an unrelated object with the same handle.

Use callbacks from one long-lived isolate, normally the main isolate. Don't pass
callbacks to Rust from `Isolate.run`, Flutter's `compute` or any other isolate
that exits while Rust can still call it. Once the owning isolate has exited, a
call aborts the process with `Callback invoked after it has been deleted`.

Callbacks don't keep their isolate alive (`keepIsolateAlive = false`). A command
line program exits when `main` finishes, even if Rust still holds a callback,
for example a logger. If you expect a callback, keep the isolate alive yourself,
for example by awaiting a `Completer` that the callback completes. A call that
Rust queues after the main isolate has exited is dropped as the process exits.

## Hot restart

A Flutter hot restart replaces the main isolate but keeps the native library
loaded, together with every callback object and vtable that Rust holds.

- Until the new isolate passes a callback of a given type to Rust, Rust still
  calls the old isolate's listeners, and the process aborts.
- After that, Rust calls the new isolate. Handles carry a random per-isolate
  prefix on 64-bit targets, so a handle from the old isolate fails as a stale
  handle instead of reaching an object in the new one.

Release Rust-held callbacks before a hot restart, for example by disconnecting,
or use a full restart.

## Deadlocks

A synchronous Rust function, called from Dart, that blocks until a callback has
run deadlocks if that callback is delivered through a listener. The isolate's
thread is blocked in the Rust call, so the queued callback can't run. Make such
functions async, or don't wait for the callback.

## Opt-in dispatch

With `callback_dispatch = true`, the generated companion library relays every
synchronous callback instead, and the bindings keep using `Pointer.fromFunction`
for all of them. See [callback-dispatch.md](callback-dispatch.md).
