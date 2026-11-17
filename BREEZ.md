# Breez fork: v0.29.4-with-fixes

uniffi-dart on UniFFI 0.29.4, for the Breez SDK Flutter package, which is on
UniFFI 0.29.x. This is the branch the SDK pins.

Only merges land here. Every change has its own branch and arrives as a merge.
Branches that could become upstream PRs are based on upstream main
(`Uniffi-Dart/uniffi-dart`, 0.31). The rest are based on the port.

Started from upstream main at `ab977ca`.

Rule: nothing goes in unless upstream is likely to take it. The goal is to drop
the fork and use an upstream release. The 0.29 port is the only fork-only
exception, and it goes once the SDK's other bindings reach a UniFFI version
upstream supports.

## Upstream status

Checked on 2026-11-17 against `Uniffi-Dart/uniffi-dart`.

| Item | Upstream | Same as upstream? | Converges by |
|---|---|---|---|
| UniFFI 0.29.4 port (`savage-uniffi-v0.29.4`) | Fork-only, the allowed exception | No, by design | Dropped when the SDK moves to UniFFI 0.31 or later and uses upstream releases. |
| #183 roll back owned arguments | Open, chavic | Yes, at its head `ccc93d6` | Upstream merge. |
| #181 wire types in callbacks | Open, chavic | Yes, at its head `325c0ea`, plus one 0.29-only test change (`a2944e7`): a foreign trait returned through Rust comes back as a Rust-backed wrapper on 0.29 | Upstream merge. The test change goes with the port. |
| #178 `status` argument names | Open, chavic | Yes, at its head `9573490`. The merge resolutions with #181 and #183 are what upstream needs too | Upstream merge. |
| #196 `Error` inside type names | Open, ThoreKoritzius | Yes, at its head `c356e4f` | Upstream merge. |
| `savage-fix-enum-field-error-names` | Ours. Superseded by #196, not to be PR'd | Nothing of it is left: its code change is identical to #196's, and its fixture was removed (`df4ea33`), so the fork carries #196 as it stands | Nothing to converge. |
| #190 fixed-width handles | Open, chavic | **No: partial.** Only the Rust-future part (`Uint64` future handles). Its object-handle parts don't apply, since 0.29 passes objects as pointers | Take #190 unchanged once the port goes. |
| #188 u64 as `BigInt` | Open draft, chavic | Yes, at its head `f0b7a21`. The merge moves #183's tests to `BigInt` | Upstream merge. |
| #186 opt-in callback dispatch | Merged upstream | Yes (in the base) | Already upstream. |
| `savage-async-callback-listeners-on-0.29` | Ours, superseded, not to be PR'd | Its `callback_listeners` flag is removed by the always-on merge | Nothing left in the tree. |
| `savage-callback-listeners-always-on-0.29` | Ours, to be PR'd (draft in `PR-DRAFT.md` on `savage-callback-listeners-always`) | Yes. Async methods and frees only: sync callbacks wait for the relay PR | Upstream PR. |
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

## Not taken

- #191 (Rust `Display` for error messages): needs `#[uniffi::export(Display)]`
  on enums, which UniFFI 0.30 introduced. Its #183 commits are in via #183.
- #193 (u64 as `int` with the full bit pattern): #188 (BigInt) was chosen instead.
- #192 (enum return in sync callbacks): overlaps #181.
- #186 (opt-in sync callback dispatch): in upstream main, but not enabled.
  It rejects async and 32-bit targets.
