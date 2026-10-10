---
id: CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA
kind: issue
title: A step that only passes the value through replaces the input state's metadata with the context's
status: draft
priority: P2
complexity: M
area: [core/plan]
design: plan-step-state-metadata
created: 2026-10-10
github:
---
## Problem

`liquers-core/src/interpreter.rs` `apply_plan` rebuilds the state after **every** step from the
step's value and `context.get_metadata()`. Steps that only pass the value through — `Step::Info`,
`Step::Warning`, `Step::Error`, `Step::SetCwd`, `Step::Filename` all return `input.value()` in
`do_step` — therefore discard the input state's own metadata (`data_format`, type identifier,
title …) and replace it with whatever the context holds.

`specs/reference/api/DOC_08_RECIPES_PLANS.md` §Plan fields and execution documents the
replacement ("the state handed to the next step carries the context's metadata"); the gap is the
supplied input state, whose metadata the context never held.

When a plan is applied to a supplied state (`AssetManager::apply`, `Context::apply`) and such a
step comes before the first action, the action no longer sees the input's metadata. Example: a CSV
text state (`data_format: csv`) applied to a plan `[Info(..), Action(pl/slice)]` reaches `slice`
with `data_format: bin`, and polars refuses it: *"Unsupported polars data_format 'bin'"*.

## Impact

Any planner feature that emits an executed diagnostic or context step ahead of an action breaks
commands that read the input's metadata. It was hit by command aliases, whose planner emitted a
`Step::Info` before the target action; that design worked around it by moving the message to
`plan.init_steps`. Other current plans put such steps after the actions or start from a resource
step, so no user-visible failure is known today. Workaround: keep pass-through steps out of
`plan.steps` before the first action.

## Expected behaviour

A step that does not produce new data keeps the input state's metadata (or merges the context's
log into it) instead of replacing it.

## Discovery

`design/command-alias-contract` Phase 4, Step 8: `polars_commands::test_head_alias_matches_slice`
failed until the alias message moved to `init_steps`.
