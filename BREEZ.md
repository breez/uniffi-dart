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

## Not taken

- #193 (u64 as `int` with the full bit pattern): #188 (BigInt) was chosen instead.
- #192 (enum return in sync callbacks): overlaps #181.
- #186 (opt-in sync callback dispatch): in upstream main, but not enabled.
  It rejects async and 32-bit targets.
