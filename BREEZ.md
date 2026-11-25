# Breez fork: v0.29.4-with-fixes

uniffi-dart on UniFFI 0.29.4, for the Breez SDK Flutter package, which is on
UniFFI 0.29.x. This is the branch the SDK pins.

Only merges land here. Every change has its own branch and arrives as a merge.
Branches that could become upstream PRs are based on upstream main
(`Uniffi-Dart/uniffi-dart`, 0.31). The rest are based on the port.

Started from upstream main at `ab977ca`. Upstream main is merged in as it
moves (now `304bae0`).

Pushes to `origin` (github.com/breez/uniffi-dart, which the SDK installs from)
need Ross's go each time. Every update here reports whether it needs pushing.

Rule: nothing goes in unless upstream is likely to take it. The goal is to drop
the fork and use an upstream release. The 0.29 port is the only fork-only
exception, and it goes once the SDK's other bindings reach a UniFFI version
upstream supports.

## Upstream status

Last checked 2026-11-20 against `Uniffi-Dart/uniffi-dart`. The fork's base is
upstream main at `304bae0` (`ab977ca` plus #197).

Carried PRs, at the head the fork has:

| PR | Head in the fork | Carried as |
|---|---|---|
| #178 | `7b74171` | `upstream-pr-178` |
| #181 | `8d15f6a` | `savage-pr-181-on-0.29` (plus `a2944e7`) |
| #183 | `67d487c` | `upstream-pr-183` |
| #188 | `f0b7a21` | `savage-pr-188-on-0.29` |
| #190 | `697036b` | `savage-pr-190-on-0.29`, adapted (Rust-future part only) |
| #196 | `de8675a` | `upstream-pr-196` |

Watched, not carried: #191 (Rust `Display` for error messages, would fix
exceptions without a message; a reviewer saw `argument_rollback` fail on macOS
arm64 with its formatting tests in the same run), #149 (UniFFI 0.32, sets how far the 0.29 port
drifts), and any new PR touching the callback listeners or relay, custom
types, callback names or u64 callback results.

To compare against upstream:

```sh
PRS="178 181 183 188 190 196 191 149"
# The + lets a rebased PR head replace the old one. Without it, git fetch keeps the old head.
git fetch upstream main $(for p in $PRS; do printf '+pull/%s/head:refs/remotes/upstream/pr/%s ' $p $p; done)
git rev-parse --short upstream/main        # expect the base above
for p in $PRS; do echo "#$p $(git rev-parse --short upstream/pr/$p)"; done   # expect the heads above
for p in $PRS; do gh pr view $p -R Uniffi-Dart/uniffi-dart \
  --json number,state,isDraft,updatedAt --jq '"#\(.number) \(.state) draft=\(.isDraft) \(.updatedAt)"'; done
# Comments and reviews since the last check
since=2026-11-17T00:00:00Z
for p in $PRS; do
  gh api "repos/Uniffi-Dart/uniffi-dart/issues/$p/comments?since=$since" --jq ".[] | \"#$p comment \(.user.login) \(.created_at)\""
  gh api "repos/Uniffi-Dart/uniffi-dart/pulls/$p/comments?since=$since" --jq ".[] | \"#$p review comment \(.user.login) \(.path)\""
  gh api "repos/Uniffi-Dart/uniffi-dart/pulls/$p/reviews" --jq ".[] | select(.submitted_at > \"$since\") | \"#$p review \(.user.login) \(.state)\""
done
# New open PRs
gh pr list -R Uniffi-Dart/uniffi-dart --state open --json number,title,updatedAt
# What a moved head changed
git diff <old head> upstream/pr/<n>
```

When a carried head moves: point its branch at the new head, merge it here,
run the full suite and clippy, and update this file. A PR that merges upstream
moves to the merged table and becomes part of the base.

| Item | Upstream | Same as upstream? | Converges by |
|---|---|---|---|
| UniFFI 0.29.4 port (`savage-uniffi-v0.29.4`) | Fork-only, the allowed exception | No, by design | Dropped when the SDK moves to UniFFI 0.31 or later and uses upstream releases. |
| #183 roll back owned arguments | Open, chavic. Approved by reez on 2026-11-17, and the approval carried over when it was rebased onto `304bae0` on 2026-11-20 | Yes, at its head `67d487c`, a rebase of `ccc93d6` with the same patches | Upstream merge. |
| #181 wire types in callbacks | Open, chavic. Approved by reez on 2026-11-17, and the approval carried over when it was rebased onto `304bae0` on 2026-11-20 | Yes, at its head `8d15f6a` (a rebase of `325c0ea` with the same patches), plus one 0.29-only test change (`a2944e7`): a foreign trait returned through Rust comes back as a Rust-backed wrapper on 0.29 | Upstream merge. The test change goes with the port. |
| #178 `status` argument names | Open, chavic. Approved by DenisovAV on 2026-11-17, and the approval carried over when it was rebased onto `304bae0` on 2026-11-20 | Yes, at its head `7b74171`, a rebase of `9573490` with the same patches. The merge resolutions with #181 and #183 are what upstream needs too | Upstream merge. |
| #196 `Error` inside type names | Open, ThoreKoritzius | Yes, at its head `de8675a`. It was force-pushed from `c356e4f` with 112 more fixture lines and the same generator change | Upstream merge. |
| `savage-fix-enum-field-error-names` | Ours. Superseded by #196, not to be PR'd | Nothing of it is left: its code change is identical to #196's, and its fixture was removed (`df4ea33`), so the fork carries #196 as it stands | Nothing to converge. |
| #190 fixed-width handles | Open, chavic. Approved by reez on 2026-11-17, and the approval carried over when it was rebased onto `304bae0` on 2026-11-20 | **No: partial.** Only the Rust-future part (`Uint64` future handles), adapted in `b3c33dd` from #190's head `41e82ed`. Its current head `697036b` is a rebase with the same patches, so the adaptation stands. Its object-handle parts don't apply, since 0.29 passes objects as pointers. After its main commit `ca8b7ac`, #190 is test-only apart from `ce4fb6b`'s `Handle` field annotation, which the adaptation includes | Take #190 unchanged once the port goes. |
| #188 u64 as `BigInt` | Open draft, chavic | Yes, at its head `f0b7a21`, plus the local fix in the next row. The merge moves #183's tests to `BigInt` | Upstream merge. |
| `savage-u64-callback-results-bigint-on-0.29` | Local fix on top of #188 (`2d694b9`, on #188's head `f0b7a21`). Suggested on #188 (comment by dangeross, 2026-11-17), but not part of it | **No.** #188 types u64 callback results as `Object`, so implementers lose type checking. This types them as `BigInt` (`Future<BigInt>` when async). #188's own commits are untouched | If #188 adopts the suggestion: take #188's new head and drop this branch. If #188 rejects it: this becomes fork-only, which the rule above doesn't allow, so revert the merge. Dart callbacks that return `BigInt` keep compiling against `Object`, so the SDK needs no change for the revert. |
| #186 opt-in callback dispatch | Merged upstream | Yes (in the base), as changed by `savage-callback-relay`: its runtime is shared with the relay, and it wakes the isolate through a port | Already upstream; the changes go up with the relay PR. |
| `savage-async-callback-listeners-on-0.29` | Ours, superseded, not to be PR'd | Its `callback_listeners` flag is removed by the always-on merge | Nothing left in the tree. |
| `savage-callback-listeners-always-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-callback-listeners-always`) | Yes. Async methods and frees only: sync callbacks wait for the relay PR | Upstream PR. |
| `savage-callback-relay-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-callback-relay`, stacked on the listeners PR). `savage-callback-relay` was rebased onto the listeners branch's new commits on 2026-11-19, so origin has its old commits until a force push | Yes, plus dropping `uniffi_clone`, which 0.29 doesn't have | Upstream PR. |
| `savage-native-finalizers-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-native-finalizers`, stacked on the relay PR) | Yes | Upstream PR. |
| `savage-custom-type-config-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-custom-type-config`) | Yes, minus the fixture's default-value cases, which UniFFI 0.29 rejects. The merge also drops #181's custom-type recursion in callback return handling, which both PRs upstream will need | Upstream PR. |
| `savage-custom-type-config-u64-on-0.29` | Ours, from `savage-custom-type-config-u64` (`0ff09b0`, on #188 plus the custom-types branch). Goes into the custom-types PR if #188 lands first, as that PR's draft now says | Yes, minus the fixture's `Stamp` defaults, which UniFFI 0.29 rejects. The merge also threads the type helper through #183's `scoped_ffi_call` | Upstream, with the custom-types PR. If that PR lands before #188, #188 needs the same change. |
| `savage-fix-callback-name-collisions-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-fix-callback-name-collisions`), independent of the callback PRs | Yes, minus the clone member, which 0.29 doesn't have | Upstream PR. |
| `savage-record-equality-on-0.29` | Ours, to be PR'd (no draft yet). Ross asked for it on 2026-11-25 after Erdem's review of spark-sdk#1338 found that SDK types compare by identity | Yes | Upstream PR. |
| `savage-sealed-enums-on-0.29` | Ours, to be PR'd (no draft yet). Ross asked for it on 2026-11-25, from the same review: flutter_rust_bridge's freezed unions gave exhaustive switches | Yes | Upstream PR. |
| `BREEZ.md` | Fork-only | n/a | Goes with the fork. |

## Merged

| Branch | Base | What |
|---|---|---|
| `savage-uniffi-v0.29.4` | upstream main | Port to UniFFI 0.29.4. Breez-only. |
| `upstream-pr-183` | upstream main | #183: roll back owned arguments when lowering fails. |
| `savage-pr-181-on-0.29` | port | #181 (wire types in callbacks), plus one 0.29 test change: a foreign trait passed through Rust comes back as a Rust-backed wrapper. |
| `upstream-pr-178` | upstream main | #178: avoid shadowing `status` arguments. The merge resolves conflicts with #181 and #183 by keeping their call structure and applying #178's names. |
| `savage-fix-enum-field-error-names` | upstream main | Ours, superseded by #196 below and since emptied: its fixture was dropped in a later merge, and its code change is #196's. Enum variant fields keep `Error` mid-name in type names (`LnurlErrorDetails`). |
| `savage-pr-190-on-0.29` | port | Rust-future part of #190: future handles are `Uint64`, fixing the 32-bit ARM ABI. Its object-handle parts don't apply on 0.29. |
| `savage-pr-188-on-0.29` | port | #188 (draft upstream): u64 as `BigInt`, with `int` or `BigInt` accepted as input. Applies to the port unchanged. The merge resolves conflicts with #178 and #181 and moves #183's tests to `BigInt`. |
| `savage-async-callback-listeners-on-0.29` | port | Superseded by the next row. Opt-in `callback_listeners = true`: async methods, fire-and-forget sync methods and `uniffi_free` become `NativeCallable.listener`s that any thread may call (#161). |
| `savage-callback-listeners-always-on-0.29` | port | Ours, from `savage-callback-listeners-always` (upstream main, upstreamable, draft in its `PR-DRAFT.md`). Listener delivery always on, replacing the `callback_listeners` flag. Adds `uniffiCallbackBarrier()`, drops the async cancel hook, and gives handles a random per-isolate prefix for hot restart. See `docs/callback-threads.md`. The merge moves #183's handle-count tests and #178's status test to await the barrier. A later merge narrows it to async methods and frees, so sync void methods run inline again, and #178's test is back to its original form. |
| `upstream-pr-196` | upstream main | #196: name types with `Error` inside their name by their declared Dart name, in enum variant fields and in `Optional`/`Sequence` helpers. Supersedes `savage-fix-enum-field-error-names`, whose identical code change it replaces. |
| `savage-fix-enum-field-error-names` (again) | upstream main | Drops that branch's `enum-error-named-fields` fixture, so the fork carries #196 exactly. |
| `savage-callback-relay-on-0.29` | port | Ours, from `savage-callback-relay` (upstream main, upstreamable). Generates #186's companion as a callback relay for every component with callbacks. Bundled as the `<asset>_dispatch` native asset, it lets Rust call every callback from any thread: inline on the isolate's thread, queued from others, failing instead of crashing for an exited isolate. Without it, a warning names the sync callbacks that would abort. See `docs/callback-threads.md`. The SDK's Flutter package must build and bundle `uniffi_dispatch/`. |
| `savage-custom-type-config-on-0.29` | port | Ours, from `savage-custom-type-config` (upstream main, upstreamable). `[bindings.dart.custom_types.<Name>]` with Kotlin/Swift's keys (`type_name`, `imports`, `lift`/`lower`, older `into_custom`/`from_custom`) maps a custom type to a Dart type, for example the SDK's `u128` to `BigInt`. See `docs/custom-types.md`. The merge routes configured defaults through #183's scoped lowering and #188's initializer list. It also makes #181's callback return handling lower custom types with their own converter instead of the builtin's, and restores the fixture's custom-scalar callback returns, which #181 fixes (#162). |
| `savage-fix-callback-name-collisions-on-0.29` | port | Ours, from `savage-fix-callback-name-collisions` (upstream main, upstreamable, independent of the callback PRs). Each callback type's trampolines, pointers and vtable become static members of `UniffiCallbackInterface<Name>Impl`, so they can't collide with exported functions or with each other. The merge applies the same naming to the listener and relay additions (`{method}InlinePointer`, `uniffiFreeInlinePointer`, the relay's install function as an `external static`). |
| `savage-u64-callback-results-bigint-on-0.29` | `savage-pr-188-on-0.29` | Local fix on top of #188 (`2d694b9`), kept as its own branch so #188's commits stay exact. u64 results of callback and foreign-trait methods are `BigInt` instead of `Object`, so implementations are type-checked. Lowering is unchanged, so out-of-range values still fail #188's range check. Arguments, constructors and record fields keep #188's `int` or `BigInt` input. The SDK's Dart `Storage` implementations must return `BigInt`. The merge also moves #183's `ReentrantConsumer.accept` from `Object` to `BigInt`. |
| `savage-custom-type-config-u64-on-0.29` | `savage-pr-188-on-0.29` + `savage-custom-type-config-on-0.29` | Ours, from `savage-custom-type-config-u64` (upstreamable with the custom-types PR). Fixes custom types × #188: #188's u64 input helpers stop at a configured custom type and leave it to its own converter, so a type configured over `u64` (for example a timestamp as `DateTime`) is no longer an `Object` parameter run through u64 normalization. Unconfigured u64 aliases keep `int` or `BigInt` input. The helpers get the type helper passed in, as custom-type config already does, rather than a global lookup. Callback results of custom types use their own converter, as the custom-types merge already did here. The fixture adds a u64-backed `Stamp` configured as `DateTime` in every position, plus `Money` callback returns. The SDK has no u64-backed custom types, so its bindings are unchanged. The merge keeps the `BigInt` callback results above and threads the type helper through #183's `scoped_ffi_call`. |
| upstream main at `304bae0` | upstream main | #197: avoid owned strings in the streams_ext test's membership checks, for nightly clippy. Test-only. The fork's base moves from `ab977ca` to `304bae0`. |
| `savage-callback-relay-on-0.29` (again) | port | Hot-restart fixes that Ross asked for in the fork (2026-11-19), found by the SDK's hot-restart and Android reopen checks, in the branches whose code they fix. Listener branch (`b6ddfbd`): a call through a handle this isolate doesn't have, to a method that returns nothing and can't throw (a logger's `log`, an event listener's `onEvent`), is dropped with success and one warning instead of panicking in Rust, which aborted the SDK. Relay branch, rebased onto that: async void calls for an exited isolate complete with success (`fec7123`), and Rust futures wake Dart through the relay's port instead of a listener per call, so a call that outlives its isolate no longer crashes the process (`797cb0a`). Ported by merging into `savage-callback-listeners-always-on-0.29` (`af2770a`), merging that into the relay port, and cherry-picking the two relay commits. The merge here applies #190's integer poll data and #178's status names. Methods with no error type that return a value still panic, as documented. |
| `savage-callback-relay-on-0.29` (third time) | port | Several isolates per callback type (Ross approved, 2026-11-19, folded into the relay PR rather than a stacked one). With the relay bundled, callback handles come from the relay, which records each handle's owning isolate. Every call goes to that isolate's inline vtable, so two isolates can implement the same interface, for example the app and a push-notification handler, and an exited isolate's handles fail or drop at its own endpoint instead of reaching a new isolate. The owner-thread check also compares the isolate's main port, since a new isolate can reuse an exited one's address. Relay branch commits `7c95511` and `0c874b6`, cherry-picked onto the port. Without the relay, the last isolate to install a type still gets every call. |
| `savage-native-finalizers-on-0.29` | `savage-callback-relay-on-0.29` | Ours, from `savage-native-finalizers` (upstream main, upstreamable, stacked on the relay PR, approved by Ross on 2026-11-19). With the relay bundled, generated objects free their Rust side from a `NativeFinalizer`. The companion's per-type shim only queues the object, and the companion's own thread calls the UniFFI free, since a finalizer must not call into the Dart VM and a `Drop` can. So the objects of an exited isolate (hot restart, `Isolate.run`) are freed, and with the SDK's shutdown guard an orphaned SDK stops. Generation writes the companion for components with objects too, and objects implement `Finalizable`. Without the relay, a `Finalizer` as before. |
| `savage-native-finalizers-on-0.29` (again) | `savage-callback-relay-on-0.29` | The SDK's hot-restart soak found orphans that kept running when a call was in flight: an async call's Rust future holds its receiver until Dart frees it, which a dead isolate never does. Each async call now attaches a `NativeFinalizer` for its future to a `Finalizable` token, through a companion shim for each kind of future free that the component's async calls use, and detaches it before the normal free. If the isolate dies first, the companion's free thread frees the future, which cancels it and drops the receiver. Upstream branch commit `b83f012`, cherry-picked onto the port. The merge here builds the finalizer token from #190's integer future handle. |
| `upstream-pr-178`, `savage-pr-181-on-0.29`, `upstream-pr-183` (again) | upstream main, port, upstream main | chavic rebased #178, #181, #183 and #190 onto upstream main at `304bae0` on 2026-11-20, with the same patches. The merges record the new heads without changing the tree. #178 and #183 merge with `-s ours`, since this branch already has their patches and `304bae0`, and a content merge would conflict with later changes to their lines. #181's port branch merges its new head for real, bringing #197 and keeping `a2944e7`'s 0.29 test. #190's adaptation is unchanged. |
| `savage-record-equality-on-0.29` | port | Ours, from `savage-record-equality` (upstream main, upstreamable). Records and the variant classes of enums with fields get `operator ==` and `hashCode`, as in UniFFI's Kotlin, Swift and Python bindings, so they work in `Set`s, as `Map` keys and for change detection. Lists (including `Uint8List`) and maps compare by contents, deeply, through `uniffiDeepEquals` and `uniffiDeepHash` in the shared runtime. Nested records and enums use their own `==`. Objects keep identity unless they export `Eq` and `Hash`. Variants without fields are equal by type. Error enums follow Swift (value equality) rather than Kotlin, whose exception classes keep identity, so a caught error equals one built in Dart. Exported `Eq`/`Hash` on records and enums (UniFFI 0.30 and later) isn't used: equality is always structural. `copyWith` is not included. Fixture `value-equality`. The merge resolves import conflicts with custom-type config's defaults and the comment helpers. |
| `savage-sealed-enums-on-0.29` | port | Ours, from `savage-sealed-enums` (upstream main, upstreamable). Enums with fields are a `sealed class` instead of an `abstract class`, so a switch over the variant classes is exhaustive: it needs no default when it covers every variant, and one that misses a variant is a compile error, as with freezed unions. Code outside the bindings can no longer extend or implement the base class. A `default:` in a switch statement that already covers every variant gets the analyzer's `unreachable_switch_default` warning, and a trailing `_ =>` in a switch expression gets `unreachable_switch_case` (checked with Dart 3.14.1). The proc-macro fixture switches over an enum and an error enum without a default. |

## Not taken

- #191 (Rust `Display` for error messages): needs `#[uniffi::export(Display)]`
  on enums, which UniFFI 0.30 introduced. Its #183 commits are in via #183.
- #193 (u64 as `int` with the full bit pattern): #188 (BigInt) was chosen instead.
- #192 (enum return in sync callbacks): overlaps #181.
- #186's `callback_dispatch` option (worker threads for every Rust call): in the
  base but not enabled. It rejects async and 32-bit targets. The relay above
  reuses its companion without the worker threads.
