# Breez fork: v0.29.4-with-fixes

uniffi-dart on UniFFI 0.29.4, for the Breez SDK Flutter package, which is on
UniFFI 0.29.x. This is the branch the SDK pins.

Only merges land here. Every change has its own branch and arrives as a merge.
Branches that could become upstream PRs are based on upstream main
(`Uniffi-Dart/uniffi-dart`, 0.31). The rest are based on the port.

Started from upstream main at `ab977ca`.

## Merged

| Branch | Base | What |
|---|---|---|
| `savage-uniffi-v0.29.4` | upstream main | Port to UniFFI 0.29.4. Breez-only. |
| `upstream-pr-183` | upstream main | #183: roll back owned arguments when lowering fails. |
| `savage-pr-181-on-0.29` | port | #181 (wire types in callbacks), plus one 0.29 test change: a foreign trait passed through Rust comes back as a Rust-backed wrapper. |
| `upstream-pr-178` | upstream main | #178: avoid shadowing `status` arguments. The merge resolves conflicts with #181 and #183 by keeping their call structure and applying #178's names. |
| `savage-fix-enum-field-error-names` | upstream main | Ours, upstreamable. Enum variant fields keep `Error` mid-name in type names (`LnurlErrorDetails`). |
| `savage-pr-190-on-0.29` | port | Rust-future part of #190: future handles are `Uint64`, fixing the 32-bit ARM ABI. Its object-handle parts don't apply on 0.29. |
| `savage-pr-188-on-0.29` | port | #188 (draft upstream): u64 as `BigInt`, with `int` or `BigInt` accepted as input. Applies to the port unchanged. The merge resolves conflicts with #178 and #181 and moves #183's tests to `BigInt`. |

## Not taken

- #191 (Rust `Display` for error messages): needs `#[uniffi::export(Display)]`
  on enums, which UniFFI 0.30 introduced. Its #183 commits are in via #183.
- #193 (u64 as `int` with the full bit pattern): #188 (BigInt) was chosen instead.
- #192 (enum return in sync callbacks): overlaps #181.
- #186 (opt-in sync callback dispatch): in upstream main, but not enabled.
  It rejects async and 32-bit targets.
