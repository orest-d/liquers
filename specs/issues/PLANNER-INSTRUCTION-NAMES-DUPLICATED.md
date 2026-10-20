---
id: PLANNER-INSTRUCTION-NAMES-DUPLICATED
kind: issue
title: The planner instruction names are hard-coded in three crates
status: draft
priority: P3
complexity: S
area: [core/plan, lib/commands]
design:
created: 2026-10-20
github:
---
## Problem

The set of planner instruction names is hard-coded separately in `liquers-core`
(`ActionRequest::is_ns`/`is_q`, the `"v"` check in `PlanBuilder::process_action`, the `ns`
refusal in `command_metadata.rs`), in `liquers-template/src/diagnostics.rs` (`is_instruction`:
`ns | q | v`) and partly in `liquers-lib/src/utils.rs` (`ns` and `q` only). `q` and `v` are not
refused as command names (`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`).

## Impact

A new instruction, or a renamed one, has to be found in three crates; the copies already disagree
(`NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS`).

## Expected behaviour

One public list (or predicate) of planner instruction names in `liquers-core`, used by the plan
builder, the registry's reserved-name check, the template diagnostics and `liquers-lib`.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
