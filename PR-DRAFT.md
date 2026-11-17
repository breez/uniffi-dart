<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Relay callbacks from any thread through the generated companion

Stacked on #PR1 (listener delivery for async methods and frees). Review that
first: this PR is the two commits after it.

#PR1 lets Rust call async callback methods and frees from any thread. A
synchronous callback is harder. Rust reads its out-return and call status as
soon as the call returns, so a listener can't serve it, and
`Pointer.fromFunction` aborts the VM off its isolate's thread (#161). That's
exactly what a signer looks like: the Breez SDK calls an external signer's
synchronous `is_remote()` from a tokio task. There's no pure-Dart way to do it
on stable Dart. `NativeCallable.isolateGroupBound` aborts on Dart 3.14 stable
without `--experimental-shared-data`, which stable rejects, and even with the
flag its code couldn't reach the isolate's handle maps. Rust calls each vtable
slot with its exact C signature, so the blocking stub has to be generated
native code, which is what #186's companion is.

So every component with callbacks now also generates #186's companion crate,
built from the same runtime without dispatch's worker threads. When an app
bundles it, every callback goes through generated native proxies. On the
owning isolate's thread a proxy calls the Dart function inline, exactly as
`Pointer.fromFunction` did. From any other thread it queues the call and
posts to the isolate's port: calls that return a value, can throw, or are
`uniffi_clone` block until the isolate has run them, and the rest don't wait.
Apps that don't bundle it keep #PR1's behavior.

### Packaging

`uniffi_dispatch/` is a standard-library-only crate (Rust 1.85+). Its asset id
is the original asset id plus `_dispatch`, and its generated README gives the
library basename. Users build it for the same targets as the main library and
add a second `CodeAsset` in their build hook. `docs/callback-threads.md` has
the hook snippet. One companion serves all the components in a generation,
since they share one cdylib and asset id, so multi-component packages need
nothing extra.

If the asset isn't bundled, the bindings fall back to #PR1's behavior: inline
on the isolate's thread, listeners for async methods and frees, and the old
abort if Rust calls a synchronous method from another thread. A component with
synchronous callback methods prints one warning per isolate the first time it
sets up a vtable. The warning names those methods and the missing asset. I went
with the fallback rather than throwing, so packages that never call back
off-thread keep working without a new build step. A stale companion is
different: the bindings compare an ABI hash and throw a `StateError`, as
#186's dispatcher does.

### Relation to #186

This builds on #186 rather than beside it. The first commit splits #186's
runtime into a shared core and the dispatch-only parts (routes, worker calls,
close), and changes how the companion wakes the isolate in both modes. It now
posts to a `RawReceivePort` through `Dart_PostCObject`, from the same Dart API
table #186 already reads `Dart_CurrentIsolate` from, instead of calling a
`NativeCallable.listener`. Calling a listener whose isolate has exited aborts
the process. Posting to its closed port just fails. So the companion now marks
that endpoint dead and fails or completes its queued calls instead of crashing
or hanging, which matters for Flutter hot restart. The ABI hash now covers the
runtime text, as suggested in #186's review, and `next_id` no longer uses the
deprecated `fetch_update`.

Without `callback_dispatch`, the relay replaces the worker threads with
inline calls on the owner's thread, and never wraps Rust calls made from Dart.
Those wrappers are what #186's validation rejections protect, so the relay
has none of them. The fixtures cover async APIs, Rust objects, and foreign
traits passed through callbacks. External types and multiple components share
one companion by construction, but no fixture here exercises them. The native
side uses `u64` handles throughout. 32-bit Dart-side handle lowering is #190's.
`callback_dispatch` keeps one thing the relay can't do: a synchronous Rust call
made from Dart that waits for a callback on another thread.

### Exited isolates

When Rust calls back into an isolate that has exited, after a hot restart or an
`Isolate.run` that registered a callback, a synchronous call gets an unexpected
error in its call status. For a method without an error type, UniFFI turns
that into a panic on the calling Rust thread. An async call is completed with a
zeroed result and an unexpected error, so a Rust task awaiting it doesn't hang.
A call that returns nothing, or a free, is dropped. The first of these logs one
warning. A hot restart's new isolate takes each callback type over the first
time it passes a callback of that type to Rust.

The death is noticed when a post fails. Every queued call posts, rather than
coalescing wake-ups, because an isolate that dies with a wake-up outstanding
would otherwise never be noticed. Installing a vtable and a stalled
synchronous call also probe, so calls queued just before the isolate died
still complete.

### Limits

- A synchronous Rust call made from Dart that waits for another thread's
  synchronous callback deadlocks. The callback waits for the isolate, which is
  blocked in the Rust call. Before this PR it aborted. I looked at failing fast
  instead: the generated Dart could mark the isolate as busy around every
  synchronous Rust call, and the relay could fail a request that finds it
  busy. But busy doesn't mean waiting for that request. The Dart thread makes
  short synchronous calls all the time, and a tokio thread that calls back
  during one of them would fail at random, where waiting a few microseconds
  would have worked. A timeout has the same problem with slow calls. So the
  relay waits, and after two seconds it logs one warning that names the likely
  cause and `callback_dispatch`, to stderr or to logcat on Android through
  liblog.
- A callback from the isolate's own thread while Dart isn't running there, for
  example Flutter platform code calling into Rust, looks like any other thread
  and deadlocks the same way.
- Calls for an object registered by another isolate reach the isolate that
  installed the vtable last, as before, and fail there as a stale handle
  instead of aborting.
- A call that returns nothing and throws after Rust stopped waiting, from
  another thread, can't report the error to Rust. The relay logs one warning,
  and the error buffer leaks, since the companion doesn't link the original
  library's `rustbuffer_free`.
- Generated code runs in the companion, not the user's library, so users have
  to build and bundle it. I couldn't find a way around that on stable Dart.

### Testing

The test harness builds and bundles the companion whenever one is generated,
so every fixture with callbacks now runs through the relay. `callback-threads`
also runs without it, which keeps #PR1's listener path covered. The new
`callback-relay` fixture covers:

- sync calls from a `std::thread` and from a tokio task, and 32 concurrent
  callers;
- a declared error thrown on another thread;
- a void `store()` then a value `load()` from one thread, and on the Dart
  thread, which must see the stored value;
- a relayed callback that calls into Rust, which calls back inline;
- a foreign trait passed to a callback on another thread (`uniffi_clone`);
- async methods and frees from other threads;
- callbacks of an exited isolate failing instead of aborting, and a new
  isolate taking the type over;
- the deadlock warning, in a child process that the test kills.

Without the companion, the same fixture checks that sync callbacks run inline,
that one warning names them, and that a call from another thread still aborts.
Generator unit tests cover each slot's delivery, the ABI hash, and the warning
literal.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 45 test
targets, 312 Dart cases. `cargo clippy --workspace --all-targets -D warnings`
and `cargo fmt --check` are clean. The generated relay crates for the
`callback-relay`, `callbacks` and `dart_async` fixtures are clippy-clean with
`-D warnings` on Rust 1.85.0 and on stable. #186's generated probe passes in JIT
and AOT on both commits, and the first commit alone passes the companion's unit
tests and the JIT probe. The companion was built on Rust 1.85.0, since 1.85.1
wasn't installed.

On Android, the same branch ported to UniFFI 0.29 passes all 180 device cases,
in one release AOT build with every fixture's relay library cross-compiled and
bundled. It passes both on an arm64 emulator (API 36) and on a 32-bit-only
Galaxy A13 (API 34, Firebase Test Lab). That includes the `callback-relay`
cases and a smoke test of a real SDK whose logger is a synchronous void
callback that Rust calls from its own threads. A control, using bindings whose
relay library isn't bundled, aborts as expected on both.

Related to #161. Fixes the synchronous half of it when the companion is
bundled.
