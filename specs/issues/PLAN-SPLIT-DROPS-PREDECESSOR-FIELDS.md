---
id: PLAN-SPLIT-DROPS-PREDECESSOR-FIELDS
kind: issue
title: Plan split drops predecessor fields
status: draft
priority: P3
complexity: S
area: [core/plan]
design: predecessor-cut-equivalence
created: 2026-08-26
github:
---
## Problem

`Plan::split` (`liquers-core/src/plan.rs:246`) builds both halves with `Plan::new()` and then
copies `query`, `init_steps`, `steps`, `is_volatile`, `payload_required`, `expires`, `error`
and `dependencies`. It does not copy `frozen_cwd`, `predecessor` or `predecessor_steps`, the
three fields `plan-cwd-freeze` added.

Both halves therefore report `predecessor: None` and `frozen_cwd: None` regardless of the
plan they came from. A split half is silently un-frozen — so `cut_predecessor` on it would
fail its "must be frozen" guard, and a re-freeze against a different key would succeed where
the whole plan would have refused.

The comment above the copy list says both halves "retain query-level analysis fields", which
is now only partly true.

## Impact

None today: `Plan::split` has no production caller — the seven call sites are all in
`plan.rs`'s own `mod tests`. It is a trap for the next caller rather than a live defect,
which is why this is P3 and not higher.

## Expected behaviour

Both halves carry `frozen_cwd`. The predecessor belongs to the first half only, since it is
by definition the leading steps: the first half keeps `predecessor` and `predecessor_steps`
(clamped to its own step count), the second half gets `None`/`0`.

Better still, build both halves by cloning `self` and replacing `steps`, so a field added to
`Plan` in future is carried by construction rather than by remembering to extend a list. That
is the shape that would have prevented this.

## Discovery

Noticed while reading `plan.rs` for `PREDECESSOR-CUT-NOT-YET-EQUIVALENT`, 2026-08-26. Not
reached by any measurement; found by reading the field list against the struct.
