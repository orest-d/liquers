---
id: PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS
kind: issue
title: Plan split drops predecessor fields
status: accepted
priority: P2
complexity: S
area: [core/plan]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

`Plan::split` (`liquers-core/src/plan.rs:2246`) builds both halves with `Plan::new()` and then
copies a field list: `query`, `init_steps`, `steps`, `is_volatile`, `payload_required`,
`expires`, `error`, `dependencies`. It does not copy `frozen_cwd`, `predecessor` or
`predecessor_steps` — the three fields `plan-cwd-freeze` added — and would not copy
`prologue_steps`.

Both halves therefore report `predecessor: None` and, more consequentially, `frozen_cwd: None`.
A split half is silently **un-frozen**: `cut_predecessor` would reject it on its frozen guard,
and `freeze_cwd` would accept a re-freeze against a different key that the whole plan refuses.

The comment above the copy list says both halves "retain query-level analysis fields", which is
now only partly true.

## Impact

No production caller: the only call sites are `plan.rs`'s own `mod tests` (confirmed across the
workspace, 2026-08-26). It is a trap for the next caller, not a live defect.

Raised from P3 to P2 and taken into scope anyway, because the omission is not the interesting
part — **the field list is**. This is the third instance of one shape: a plan mutated through a
subset of coupled fields.

| Where | What went stale |
|---|---|
| `Recipe::to_plan` inserting `SetCwd` | `predecessor_steps`, until `plan-cwd-freeze` bumped it — a cut ran the predecessor's action twice |
| `Plan::freeze_cwd_with` | the cursor used to resolve `predecessor` — `PREDECESSOR-CUT-NOT-YET-EQUIVALENT` Cause 1 |
| `Plan::split` | `frozen_cwd`, `predecessor`, `predecessor_steps` — this issue |

Two of the three shipped.

## Expected behaviour

Build each half from `self.clone()` and replace only what differs, so a field added to `Plan`
later is carried by construction, and a field that must *not* be carried has to be cleared
deliberately — in the diff, where a reviewer sees it.

What each half carries, which is **not** what an earlier draft of this issue proposed:

| Field | First half | Second half |
|---|---|---|
| `frozen_cwd` | carried | carried |
| `predecessor`, `predecessor_steps` | cleared | cleared |
| `prologue_steps` | carried, clamped to its own step count | `0` |

The earlier draft said the first half should keep `predecessor` and `predecessor_steps`
"clamped to its own step count". That is wrong. Measured, `split_index == predecessor_steps` on
every shape tried, prologue included:

```
fetch/expensive/render           steps=3 split_index=2 predecessor_steps=2
fetch/expensive/render/out.txt   steps=4 split_index=2 predecessor_steps=2
-R/./a.txt/-/fetch/render        steps=3 split_index=2 predecessor_steps=2
recipe cwd=a/c, 4-action query   steps=5 split_index=3 predecessor_steps=3
```

The first half **is** the predecessor's steps. Keeping the field would give it
`predecessor_steps == steps.len()`, which passes `cut_predecessor`'s range guard and cuts every
step into a boundary recomputing the same thing — a degenerate wrapper. The first half's
genuine predecessor is one level deeper, and `split` has no registry to build it, so `None` is
the honest answer.

Also worth adding, and cheap: a `debug_assert`-backed consistency check —
`prologue_steps <= steps.len()`, and `prologue_steps <= predecessor_steps <= steps.len()` when
`predecessor.is_some()` — called after `build`, after `Recipe::to_plan`'s insert, after `split`
and after `cut_predecessor`. It would have caught the double-execution bug at its source rather
than through a failing evaluation two layers away.

## Discovery

Noticed while reading `plan.rs` for `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`, 2026-08-26; found by
reading the field list against the struct, not by any measurement. Filed P3 and out of scope,
then brought into `predecessor-cut-equivalence` at the author's direction as a correctness
issue. See that design's `solution.md` §1b.
