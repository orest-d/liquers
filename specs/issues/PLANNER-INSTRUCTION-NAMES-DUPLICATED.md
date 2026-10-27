---
id: PLANNER-INSTRUCTION-NAMES-DUPLICATED
kind: issue
title: The planner instruction names are hard-coded in three crates
status: draft
priority: P3
complexity: L
area: [core/query, core/plan, lib/commands]
design: planner-instruction-names-public
created: 2026-10-20
github:
---
## Problem

The set of planner instruction names is hard-coded separately in `liquers-core`
(`ActionRequest::is_ns`/`is_q`, the `"v"` check in `PlanBuilder::process_action`, the `ns`
refusal in `command_metadata.rs`), in `liquers-template/src/diagnostics.rs` (`is_instruction`:
`ns | q | v`, and a literal `Some("ns")` in `template_commands`) and partly in `liquers-lib/src/utils.rs` (`ns` and `q` only). `q` and `v` are not
refused as command names ([`COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED`](COMMAND-NAMES-Q-AND-V-ARE-NOT-RESERVED.md)).

## Problem Example

`find_next_presets("text-Hello/uppercase/v", …)` (`liquers-lib`) skips `ns` and `q` but not `v`, so
it takes `v` as the last command and offers no presets. The template diagnostics and the planner
both know `v`. A fourth instruction (`cache`, proposed by `query-cache-directives`) would be reported
as an unknown command by the template diagnostics until someone found that copy. Should be: every
crate calls one `liquers_core::query::is_planner_instruction`.

## Impact

A new instruction, or a renamed one, has to be found in three crates; the copies already disagree
(`NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS`).

## Expected behaviour

One public list (or predicate) of planner instruction names in `liquers-core`, used by the plan
builder, the registry's reserved-name check, the template diagnostics and `liquers-lib`.

Complexity corrected from `S` to `L` on 2026-10-26: the change crosses three crates and adds public
API (`DOCS_STRUCTURE_GUIDE.md` §4.5).

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
