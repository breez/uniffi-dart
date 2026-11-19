# Callbacks from other threads

Rust may call a Dart callback interface or foreign trait from any thread: a
`std::thread`, a tokio worker or a thread owned by a C library. A callback
created with `Pointer.fromFunction` can only run on the thread of the isolate
that created it. A call from anywhere else aborts the process with
`Cannot invoke native callback outside an isolate` (#161).

The bindings handle this in one of two ways, depending on whether the app
bundles the generated callback relay library:

| Callback | With the relay library | Without it |
|---|---|---|
| sync method that returns a value or can throw, `uniffi_clone` | inline on the isolate's thread. From another thread, queued to the isolate, and Rust waits for the result | inline on the isolate's thread. Aborts from another thread |
| sync method that returns nothing and can't throw | inline on the isolate's thread. From another thread, queued, and Rust doesn't wait | inline on the isolate's thread. Aborts from another thread |
| async method | starts inline on the isolate's thread. From another thread, queued | `NativeCallable.listener` from any thread, starting on a later event-loop turn |
| `uniffi_free` | inline on the isolate's thread. From another thread, queued | `NativeCallable.listener` from any thread, on a later event-loop turn |

"The isolate" is the one that owns the callback type's vtable (see
[Isolates](#isolates)). Bundle the relay library whenever Rust may call a
synchronous callback from another thread.

## The callback relay library

Whenever a component has callback interfaces or foreign traits, generation also
writes a Rust crate to `uniffi_dispatch/`, next to the Dart files. It uses only
the Rust standard library and builds with Rust 1.85 or later. Its generated
`README.md` records the two names you need:

- the library basename, `<cdylib name>_dart_dispatch`, and
- the native asset id, the original library's asset id plus `_dispatch`, for
  example `package:my_app/uniffi:my_lib_dispatch`.

Build it for the same targets as the original library, for example:

```sh
cargo build --release --manifest-path path/to/bindings/uniffi_dispatch/Cargo.toml
# Cross compiling: add --target, or build it with cargo-ndk like the main library.
```

Bundle it from the package's build hook, next to the original library's
asset:

```dart
import 'package:code_assets/code_assets.dart';
import 'package:hooks/hooks.dart';

void main(List<String> args) async {
  await build(args, (input, output) async {
    // ... the CodeAsset for the original library, `uniffi:my_lib` ...
    output.assets.code.add(
      CodeAsset(
        package: input.packageName,
        name: 'uniffi:my_lib_dispatch',
        linkMode: DynamicLoadingBundled(),
        file: input.packageRoot.resolve('native/libmy_lib_dart_dispatch.so'),
      ),
    );
  });
}
```

The bindings and the library carry the same hash of the relay's ABI. If they
differ, the first callback type that sets up its vtable throws a `StateError`,
so rebuild the library whenever you regenerate the bindings.

### Without the library

If the asset isn't bundled, the bindings work as they did before, except that
async methods and frees use listeners. The first time a component that has
synchronous callback methods sets up a vtable, it prints one warning per
isolate, naming those methods and the missing asset.

## How the relay works

On the owning isolate's thread, a relayed callback runs inline, exactly as
`Pointer.fromFunction` would, including when Rust calls it from inside a
callback that the relay is running. From any other thread the relay queues
the call and posts a message to the isolate's port. The isolate runs queued
calls in order when it gets the message. A Rust thread that needs a result
blocks until then. Calls from one Rust thread run in the order it made them,
so a `set()` followed by a `get()` sees the new value.

The relay also carries the wake-ups of Rust futures that Dart awaits, in place
of a listener per async call. It posts each one to the awaiting isolate's port,
and drops it if that isolate has exited.

## When listener calls run

Without the relay library, a listener call is queued to the isolate and runs on
a later event-loop turn, even when Rust makes the call on the isolate's own
thread, during a synchronous Rust function. So an async method starts, and a
dropped callback object is removed from the bindings' handle map, slightly
after the Rust call returns.

To wait for queued calls, for example to check handle counts in a test, await
`uniffiCallbackBarrier()`. It completes after every callback that was queued to
the isolate before the call, through a listener or the relay. For an async
method it waits until the method has started, not until its future has
completed.

```dart
dropSinkOnThread(sink: Sink()); // Rust drops the sink on a worker thread.
await uniffiCallbackBarrier();
// The sink's handle has been released.
```

## Async methods and cancellation

Rust passes a completion callback, which may be called from any thread, and
pre-fills the method's out-parameter. The bindings never write that
out-parameter, because the Dart method may run after Rust's call has returned.
If Rust drops the future, for example on a timeout, the Dart method still runs
to completion. Rust ignores the late result.

## Isolates

UniFFI keeps one vtable per callback type for the whole process. An isolate
installs its vtable the first time it passes a callback of that type to Rust,
replacing any earlier one. From then on, Rust calls of that type reach that
isolate, including calls for objects registered by other isolates. On 64-bit
targets those fail with a stale-handle error, or are dropped for methods that
return nothing and can't throw (see
[Exited isolates and hot restart](#exited-isolates-and-hot-restart)). On 32-bit
targets they can reach an unrelated object with the same handle. Use callbacks
from one long-lived isolate, normally the main isolate.

Callbacks don't keep their isolate alive. A command line program exits when
`main` finishes, even if Rust still holds a callback. If you expect a callback,
keep the isolate alive yourself, for example by awaiting a `Completer` that the
callback completes. Calls that Rust queues after the main isolate has exited
are dropped as the process exits. An async Rust function that Dart is awaiting
does keep the isolate alive until it finishes.

### Exited isolates and hot restart

A Flutter hot restart replaces the main isolate but keeps the native library
loaded, together with every callback object and vtable that Rust holds. The
same happens when an isolate from `Isolate.run` or `compute` exits while Rust
still holds its callbacks.

With the relay library, calls for an isolate that has exited don't crash. A
call to a method that returns nothing and can't throw, sync or async, is
dropped as if it had succeeded. Any other call fails with an unexpected error:
a synchronous call returns one to Rust, and an async call completes with one.
For a method without an error type, UniFFI turns that error into a panic on
the calling Rust thread. The relay logs one warning. After a hot restart,
the new isolate takes over each callback type the first time it passes a
callback of that type to Rust. Handles carry a random per-isolate prefix on
64-bit targets, so a handle from the old isolate misses in the new isolate
instead of reaching an object there.

A call through an old handle to a method that returns nothing and can't throw,
such as a logger's `log` or an event listener's `onEvent`, is dropped: Rust
sees it succeed, and the bindings print one warning per isolate. Any other call
fails with an unexpected error. For a method without an error type, UniFFI
turns that error into a Rust panic, which ends the process in a build with
`panic = "abort"`. Give such methods an error type, or release their callbacks
before the restart.

An async Rust function that Dart called and that is still running when its
isolate is replaced, or killed, finishes without calling into it: the relay
drops its wake-up. Nothing frees the finished Rust future, so its result stays
allocated.

An async callback that is still running in an isolate when it exits never
completes, so the Rust future awaiting it waits forever. Rust code that awaits
such callbacks one at a time, for example to deliver events to listeners, stops
delivering after the stuck call.

Without the relay library, a listener call for an exited isolate aborts the
process with `Callback invoked after it has been deleted`. That includes the
wake-up of an async Rust function that was still running when its isolate was
replaced or killed. Release Rust-held callbacks and let async Rust calls finish
before a hot restart, for example by disconnecting, or use a full restart.

## Deadlocks

A Rust thread that calls a synchronous callback through the relay waits until
the owning isolate runs it. If that isolate is itself blocked in a synchronous
Rust call that waits for the calling thread, the two wait for each other. After
waiting two seconds, the relay logs a warning that points here (to stderr, or
to logcat on Android), and keeps waiting. The same happens if non-Dart code on
the isolate's own thread, such as Flutter platform code, calls into Rust while
Dart isn't running there and Rust then calls a callback: the relay can't tell
that thread apart from any other.

Make such Rust calls async, so the isolate's event loop stays free, or use
`callback_dispatch` below. A synchronous Rust function that blocks until an
async callback has completed deadlocks in the same way.

## Opt-in dispatch

With `callback_dispatch = true`, the companion also runs every Rust call made
from Dart on a worker thread while the isolate keeps running callbacks, so a
synchronous Rust call can wait for a callback made on another thread. It
supports synchronous APIs only, and Flutter Android builds must exclude
`armeabi-v7a`. See [callback-dispatch.md](callback-dispatch.md).
