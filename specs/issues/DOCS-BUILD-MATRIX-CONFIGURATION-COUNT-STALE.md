---
id: DOCS-BUILD-MATRIX-CONFIGURATION-COUNT-STALE
kind: issue
title: CLAUDE.md states the build matrix has 11 configurations; it runs 20
status: closed
priority: P3
complexity: S
area: [docs, build]
design: documentation-currentness-small-fixes
created: 2026-09-03
github:
---
## Problem

`CLAUDE.md` §"Feature matrix" says:

> `scripts/check-build-matrix.sh` checks every configuration, library **and test targets**, plus the
> wasm32 target and `liquers-store`'s feature split:
>
> ```bash
> bash scripts/check-build-matrix.sh          # 11 configurations, ~cargo check cost
> ```

The script runs **20** at HEAD:

| Arm | Count | Source |
|---|---|---|
| `liquers-lib` | 7 | `LIB_CONFIGS` (line 47) |
| `liquers-core` | 4 | `CORE_CONFIGS` (line 57) |
| `liquers-store` | 6 | `STORE_CONFIGS` (line 64) |
| `liquers-axum` | 1 | `check liquers-axum ""` (line 104) |
| `store-conformance`, both crates | 2 | lines 110-111 |

11 was presumably right when the line was written — `LIB_CONFIGS` plus `CORE_CONFIGS` is exactly 11
— and `STORE_CONFIGS` and the three standalone checks were added afterwards without the comment
following.

## Impact

Small but real, and it misleads exactly the reader the guide is written for. The script already
prints `All ${total} configurations OK.` (line 121), so anyone who runs it sees the true number; the
harm is to anyone reasoning about coverage *without* running it — deciding whether the matrix is
worth the wait, or reviewing a change to the script and checking the count did not drift.

Observed concretely: during the Phase 4 review of `SIDECAR-COLLIDING-KEYS`, one reviewer repeated
the documented 11 and confirmed it by adding only the first two arrays, and a second arrived at 17
by counting all three arrays but missing the three standalone `check` calls. Two independent
readings, two different wrong answers, both traceable to the stale figure.

## Expected behaviour

Either drop the count from `CLAUDE.md` and point at the script's own total, or state 20 and add a
note next to the arrays in `scripts/check-build-matrix.sh` that the guide carries a count.
Preferring the first: a number in prose that duplicates a number the program computes will drift
again.

## Discovery

Found on 2026-09-03 while writing the Phase 4 implementation plan for `SIDECAR-COLLIDING-KEYS`,
which cites the matrix as its final validation step. Counting the arrays to check the plan's own
wording did not reproduce the documented figure.

## Resolution

Closed on 2026-09-04. `CLAUDE.md` no longer duplicates a configuration count and directs readers
to `check-build-matrix.sh`'s computed final total instead.
