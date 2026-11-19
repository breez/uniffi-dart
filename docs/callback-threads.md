# Callbacks from other threads

Rust may call a Dart callback interface or foreign trait from any thread: a
`std::thread`, a tokio worker or a thread owned by a C library. A callback
created with `Pointer.fromFunction` can only run on the thread of the isolate
that created it. A call from anywhere else aborts the process with
`Cannot invoke native callback outside an isolate` (#161).

The generated bindings deliver async methods and frees through
[`NativeCallable.listener`](https://api.dart.dev/dart-ffi/NativeCallable/NativeCallable.listener.html),
which accepts calls from any thread:

| Callback | Delivery | From any thread |
|---|---|---|
| async method | listener | yes |
| `uniffi_free` (Rust dropped the callback object) | listener | yes |
| sync method | `Pointer.fromFunction` | only the isolate's own thread |
| `uniffi_clone` | `Pointer.fromFunction` | only the isolate's own thread |

A sync method runs inline, while Rust's call waits, so calls on the isolate's
own thread keep their order. Calling one from another thread still aborts.

## When listener calls run

A listener call is queued to the isolate and runs on a later event-loop turn.
This is true even when Rust makes the call on the isolate's own thread, during a
synchronous Rust function. So:

- An async method starts after the Rust call that made it has returned. Rust
  only sees its result by awaiting the future, so nothing that awaits it is
  affected.
- A callback object that Rust drops is removed from the bindings' handle map
  slightly later.

The VM runs an isolate's listener calls in the order they were made. To wait for
calls that are already queued, for example to check handle counts in a test,
await `uniffiCallbackBarrier()`. It completes after every callback that was
queued before the call. For an async method it waits until the method has
started, not until its future has completed.

```dart
dropSinkOnThread(sink: Sink()); // Rust drops the sink on a worker thread.
await uniffiCallbackBarrier();
// The sink's handle has been released.
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
targets those fail with a stale-handle error, or are dropped for methods that
return nothing and can't throw (see [Hot restart](#hot-restart)). On 32-bit
targets they can reach an unrelated object with the same handle.

Use callbacks from one long-lived isolate, normally the main isolate. Don't pass
callbacks to Rust from `Isolate.run`, Flutter's `compute` or any other isolate
that exits while Rust can still call it. Once the owning isolate has exited, a
listener call aborts the process with `Callback invoked after it has been
deleted`.

Listeners don't keep their isolate alive (`keepIsolateAlive = false`). A command
line program exits when `main` finishes, even if Rust still holds a callback. If
you expect a callback, keep the isolate alive yourself, for example by awaiting
a `Completer` that the callback completes. A call that Rust queues after the
main isolate has exited is dropped as the process exits.

## Hot restart

A Flutter hot restart replaces the main isolate but keeps the native library
loaded, together with every callback object and vtable that Rust holds.

- Until the new isolate passes a callback of a given type to Rust, an async
  call or free of that type still targets the old isolate's listeners, and the
  process aborts.
- After that, Rust calls the new isolate. Handles carry a random per-isolate
  prefix on 64-bit targets, so a handle from the old isolate misses in the new
  isolate instead of reaching an object there.
- A call through an old handle to a method that returns nothing and can't
  throw, such as a logger's `log` or an event listener's `onEvent`, is dropped:
  Rust sees it succeed, and the bindings print one warning per isolate. Any
  other call fails with an unexpected error. For a method without an error
  type, UniFFI turns that error into a Rust panic, which ends the process in a
  build with `panic = "abort"`. Give such methods an error type, or release
  their callbacks before the restart.
- An async Rust function that Dart called and that is still running when its
  isolate is replaced wakes the old isolate's listener when it finishes, which
  crashes the process. The same applies to an isolate that is killed.

Release Rust-held callbacks and let async Rust calls finish before a hot
restart, for example by disconnecting, or use a full restart.

## Deadlocks

A synchronous Rust function, called from Dart, that blocks until an async
callback has completed deadlocks. The isolate's thread is blocked in the Rust
call, so the queued call can't run. Make such functions async.

## Opt-in dispatch

With `callback_dispatch = true`, the generated companion library relays every
synchronous callback, including the free, and the bindings keep using
`Pointer.fromFunction` for them. It doesn't support async methods. See
[callback-dispatch.md](callback-dispatch.md).
