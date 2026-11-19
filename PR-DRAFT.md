<!-- Draft PR description. Not for merging: remove this file before opening the PR. -->

# Relay callbacks from any thread through the generated companion

Stacked on #PR1 (listener delivery for async methods and frees). Review that
first: this PR is the six commits after it.

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
`Isolate.run` that registered a callback, a call to a method that returns
nothing and can't throw is dropped as if it had succeeded, sync or async, and
so is a free. Any other synchronous call gets an unexpected error in its call
status, and any other async call is completed with a zeroed result and an
unexpected error, so a Rust task awaiting it doesn't hang. For a method without
an error type, UniFFI turns that error into a panic on the calling Rust thread.
The first of these logs one warning. A hot restart's new isolate doesn't take
anything over: its callbacks work as soon as it passes them to Rust, and the
old isolate's keep failing or being dropped (see below).

The third commit adds the async half of the dropping. Before it, an async
method that returns nothing was completed with an unexpected error. For a
method with no error type that's a Rust panic, which in a `panic = "abort"` app
ended the process at the first event after a hot restart. Such methods get
their own delivery kind, so the ABI hash changes. #PR1 drops the same kinds of
call when they reach a new isolate through an old handle, which is what
happens without the companion.

The death is noticed when a post fails. Every queued call posts, rather than
coalescing wake-ups, because an isolate that dies with a wake-up outstanding
would otherwise never be noticed. Installing a vtable and a stalled
synchronous call also probe, so calls queued just before the isolate died
still complete.

### Async Rust calls that outlive their isolate

An async Rust function that Dart awaits wakes Dart through a continuation
callback, which `uniffiRustCallAsync` creates as a `NativeCallable.listener`
for each call. If the isolate is replaced by a hot restart, or killed, while
the function is running, Rust later wakes that listener from its own thread,
and the VM aborts with `Callback invoked after it has been deleted`. The fourth
commit passes Rust the companion's `uniffi_dispatch_continue` instead, when the
companion is bundled. The callback data carries the isolate's endpoint and the
isolate's id for the future. The companion posts each wake-up to the isolate's
relay port, where the bindings look the future up and poll it again, and drops
it if the isolate has exited. The port keeps the isolate alive while a future
is pending, as the listener did. Without the companion, async calls keep the
listener.

### Several isolates

UniFFI keeps one vtable per callback type for the whole process, and an isolate
installs its own the first time it passes a callback of that type to Rust. So
with plain vtables, the last isolate to do that gets every call of the type,
including calls for other isolates' objects, which fail there. Two isolates
can't implement the same callback interface, for example an app's main isolate
and a background isolate that handles push notifications.

The sixth commit routes each call to the isolate that owns its object. With
the companion bundled, callback handles come from the companion, which records
the owning isolate's endpoint for each one. Each callback type keeps an inline
vtable per isolate, and the proxies pick one by the call's handle, which is
always the first argument. Rust still sees one vtable. A free also drops the
handle's record. The handles come from one process-wide counter, so they're
unique across isolates, never reused, and small enough for the pointer that
carries them on 32-bit targets. That makes #PR1's random handle prefix
unnecessary when the companion is bundled, and it keeps working without it.

An exited isolate keeps its vtables, so calls through its handles still reach
its endpoint, where they fail or are dropped as above. That makes an existing
check matter more, so the fifth commit tightens it first. Deciding whether a
call is on the owning isolate's thread compared isolate addresses, and a new
isolate can reuse an exited one's address, which would run the exited
isolate's callback inline and abort. It now also compares the isolate's main
port, from the same Dart API table, which is never reused.

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
- A callback object stays with the isolate that passed it to Rust. If Rust
  hands it to code running in another isolate, its calls still go to the
  first one.
- Without the companion, the last isolate to install a callback type's vtable
  gets every call of that type, as before.
- An exited isolate's vtables and handle records stay with the companion for
  the life of the process. They're small, but each hot restart adds one entry
  per callback type.
- An async callback that is still running in an isolate when it exits never
  completes, so the Rust future awaiting it waits forever. Rust code that
  awaits callbacks one at a time, such as an event loop delivering to
  listeners, stops there. The companion could complete such calls when it
  finds the endpoint dead, but that means routing every completion through it,
  so I've left it for a follow-up.
- A Rust future whose awaiting isolate has exited is never freed, since
  nothing is left to call its free function.
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
- callbacks of an exited isolate failing instead of aborting, with sync and
  async void calls dropped as if they had succeeded, including after a new
  isolate has registered its own callback of the same type;
- two live isolates implementing the same callback interface, each getting
  its own sync, void and async calls from std and tokio threads, and the
  survivor still working after the other exits. Before the sixth commit, the
  first isolate's calls failed;
- an async Rust call that finishes on a tokio worker after its isolate was
  killed, which aborted with `Callback invoked after it has been deleted`
  before the fourth commit;
- a command line program whose only pending work is an async Rust call, in a
  child process, which must not exit before the call finishes;
- the deadlock warning, in a child process that the test kills.

Without the companion, the same fixture checks that sync callbacks run inline,
that one warning names them, and that a call from another thread still aborts.
Generator unit tests cover each slot's delivery, the ABI hash, and the warning
literal.

`cargo test --workspace` passes on macOS arm64 with Dart 3.14.0: all 45 test
targets, 318 Dart cases. `cargo clippy --workspace --all-targets -D warnings`
and `cargo fmt --check` are clean. The generated relay crates for the
`callback-relay`, `callbacks` and `dart_async` fixtures are clippy-clean with
`-D warnings` on Rust 1.85.0 and on stable. #186's generated probe passes in JIT
and AOT on both commits, and the first commit alone passes the companion's unit
tests and the JIT probe. The companion was built on Rust 1.85.0, since 1.85.1
wasn't installed.

On Android, the same branch ported to UniFFI 0.29, before the last two commits,
passes all 180 device cases,
in one release AOT build with every fixture's relay library cross-compiled and
bundled. It passes both on an arm64 emulator (API 36) and on a 32-bit-only
Galaxy A13 (API 34, Firebase Test Lab). That includes the `callback-relay`
cases and a smoke test of a real SDK whose logger is a synchronous void
callback that Rust calls from its own threads. A control, using bindings whose
relay library isn't bundled, aborts as expected on both.

Related to #161. Fixes the synchronous half of it when the companion is
bundled.
