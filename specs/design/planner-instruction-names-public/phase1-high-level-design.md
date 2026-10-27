# Phase 1: High-Level Design — planner-instruction-names-public

## Purpose

`ns`, `q` and `v` are planner instructions: the plan builder intercepts them before any command
lookup (`RECIPES_AND_PLANS.md` §Planner instructions). Every tool that walks a query's actions and
asks "is this a command?" must skip them. Today each crate keeps its own copy of the names, and the
copies already disagree. A new instruction (`query-cache-directives` proposes `cache`) would have
to be found in every crate. `liquers-core` should own one public list and predicate, and every
consumer should call it.

## Problem Example

Verified at HEAD (cd6d5da):

| Crate | Where | Names |
|---|---|---|
| `liquers-core` | `query.rs` `ActionRequest::is_ns` / `is_q`; `plan.rs` `process_action` (`action_request.name == "v"`), `append_action` (`"ns"`); `command_metadata.rs` `CommandMetadata::check` (`"ns"`) | `ns`, `q`, `v` in four places |
| `liquers-template` | `src/diagnostics.rs` `is_instruction` (`matches!(name, "ns" \| "q" \| "v")`); `template_commands` (`Some("ns")`) | `ns`, `q`, `v` |
| `liquers-lib` | `src/utils.rs` `find_next_presets` (`!a.is_ns() && !a.is_q()`) | `ns`, `q` — `v` missing |

So `find_next_presets("text-Hello/uppercase/v", …)` offers no presets
(`NEXT-PRESETS-MISREAD-PLANNER-INSTRUCTIONS`). A new `cache` instruction would be reported by the
template diagnostics as an unknown command until someone found that copy.

Should be: `liquers_core::query::is_planner_instruction("v")` is `true`, and the template
diagnostics, the presets and the planner all answer from it.

## Scope and Acceptance Criteria

- **AC-1** One public list
  WHEN a crate outside `liquers-core` needs the instruction names
  THEN it reads `liquers_core::query::PLANNER_INSTRUCTIONS` (`["ns", "q", "v"]`) or calls
  `liquers_core::query::is_planner_instruction(name)` / `ActionRequest::is_planner_instruction()`
- **AC-2** The planner and the reserved-name check use it
  WHEN `plan.rs` and `CommandMetadata::check` decide whether a name is an instruction
  THEN they use the same constants; there is no string literal `"ns"`, `"q"` or `"v"` used as an
  instruction name outside `query.rs`
- **AC-3** Template diagnostics use it
  WHEN `validate_template` checks `{{$ ns-lib/cmd/v}}` or `{{$ q}}`
  THEN `v`, `q` and `ns` are not reported as unknown commands (as today), and
  `liquers-template/src/diagnostics.rs` has no `is_instruction` of its own
- **AC-4** Presets use it
  WHEN `find_next_presets` skips instructions
  THEN it calls the core predicate (the private helper from `next-presets-follow-planner-rules` is
  removed)
- **AC-5** Ordinary names are not instructions
  WHEN `is_planner_instruction` is asked about `nsx`, `qq`, `value`, `V`
  THEN it answers `false`

Non-goals: the namespace rule (which `ns` applies to which action). It is also duplicated, in
`find_next_presets` and the template diagnostics, and is filed as
`TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY`. Also out of scope: resource header
instructions (`-R-meta/…`), which have their own table in `parse`/`plan`; adding `cache`.

## Core Interactions

Query (`ActionRequest` predicates), Plan (`process_action`, `append_action`), Commands
(`CommandMetadata::check`), template diagnostics, lib presets.

## Crate Placement

`liquers-core` owns the list, beside `ActionRequest`, which `liquers-template` and `liquers-lib`
already import. Both depend on `liquers-core` directly, so no dependency is added.

## Documentation Intent

- Reference: extend `RECIPES_AND_PLANS.md` §Planner instructions (the public list) and
  `DOC_02_QUERY_LANGUAGE_REFERENCE.md` §Planner instructions (one sentence);
  `TEMPLATE_LANGUAGE.md` where diagnostics are described (instructions come from core).
- Guide: none.
- Documents to update: the three above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` items to `liquers-core` (rule 4) and changes three
  crates (rule 6).
- **Leading issue:** Proposed resolution - public shape and name.
- **Explanation:** the change is mechanical once `reserved-instruction-command-names` has introduced
  the crate-private constants. Phases 2-4 follow the recommended shape. It is shown for review
  because it is new public API.
- **Open questions:**
  1. **Proposed resolution - public shape and name.** **Recommended:** in `liquers_core::query`,
     `pub const PLANNER_INSTRUCTIONS: [&str; 3]` (the reference's term; renamed from the crate-private
     `RESERVED_ACTION_NAMES`), `pub fn is_planner_instruction(name: &str) -> bool`, and
     `ActionRequest::is_planner_instruction` / `is_v` made `pub`. Alternatives: predicate only, or
     keeping `RESERVED_ACTION_NAMES` as the public name (it names one use, not what the names are).

## Design Dependencies

- `reserved-instruction-command-names` — **requires** (finished Phase 4, not edited; ready,
  eligible). It introduces the crate-private `NS_INSTRUCTION`, `Q_INSTRUCTION`, `V_INSTRUCTION`,
  `RESERVED_ACTION_NAMES` and `ActionRequest::is_v`, and moves `plan.rs` and `CommandMetadata::check`
  onto them. This design makes them public. If it has not landed when this starts, step 1 here does
  its Phase 4 steps 1-3 first, as written there. Overlap exclusion E1 kept the two apart: that one
  is eligible for automatic fixing, this one is not.
- `next-presets-follow-planner-rules` — **requires** (soft): it rewrites `find_next_presets` with a
  private `is_planner_instruction` helper. This design swaps that helper for the core predicate. If
  this lands first, that design calls the core predicate directly.
- `query-cache-directives` — **overlaps**: proposes a fourth instruction, `cache`. Whichever lands
  second adds it to `PLANNER_INSTRUCTIONS` (the array length changes; the predicate is unaffected).
- `TEMPLATE-DIAGNOSTICS-RESOLVE-NAMESPACES-FROM-THE-WHOLE-QUERY` (issue filed with this design, no
  design) — **overlaps**: the same file in `liquers-template`, a different function. No order.

## Consolidated Findings

- The copies differ in shape (methods, a literal compare, `matches!`, a template-AST literal
  `Some("ns")`), so the predicate takes `&str`. The method form is a convenience on top of it.
  `NS_INSTRUCTION` must be public for `template_commands`, which reads namespaces from the `ns`
  action. Its union-of-namespaces logic is the new issue's concern.
- `liquers-py` and `liquers-web` declare commands through `CommandMetadata::check`, so they need no
  copy.
