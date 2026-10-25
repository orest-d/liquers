---
id: NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS
kind: issue
title: find_next_presets reads planner instructions differently from the planner
status: draft
priority: P3
complexity: S
area: [lib/commands]
design:
created: 2026-10-20
github:
---
## Problem

`find_next_presets` (`liquers-lib/src/utils.rs`) finds the query's last command by skipping `ns`
and `q` but not `v`, so a query ending in `v` has no presets. It also computes the active namespaces
as the union of every `ns` in the segment, while the planner uses only the last `ns` before the
action (`plan.rs` `namespaces_for_query`, `Query::last_ns`).

## Impact

Next-command presets are missing after a trailing `v`, and the `ns-` injection decision can
differ from what the planner would resolve.

## Expected behaviour

Skip `v` too, and use the planner's namespace rule (or a shared helper, see
`PLANNER-INSTRUCTION-NAMES-DUPLICATED`).

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
