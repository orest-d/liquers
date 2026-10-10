---
id: CONTEXT-TITLE-LOST-ACROSS-PREDECESSOR-BOUNDARY
kind: issue
title: A title or description set by a command in a cut predecessor does not reach the final asset
status: draft
priority: P3
complexity: M
area: [core/context, core/plan]
design: context-title-predecessor-inheritance
created: 2026-10-06
github:
---
## Problem

`Context::set_title` / `set_description` write to the asset the command runs in. When a query is
evaluated without an input state, `finalize_plan` cuts the outermost cacheable prefix off as a
predecessor boundary, which runs as its own asset. In `titled/retitled` the `titled` step therefore
sets its title and description on the *predecessor* asset; the final asset sees only what
`retitled` sets (`T2` / empty), not `T2` / `D`. Applied to an input state the plan stays expanded
and both steps share one context, so the same chain yields `T2` / `D`.

Found while implementing `design/context-title-description/`, whose Phase 3 assumed the steps
always share a context. The test `a_later_step_in_the_same_query_wins` therefore uses `apply` with
an input state, and `DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md` documents the caveat.

## Impact

Whether a command's description survives depends on whether it is the last action of the chain
and on whether the caller passed an input state — neither visible to the command author.

## Update, 2026-10-10 (`plan-step-state-metadata`)

The cut plan's `Step::Evaluate` now hands the predecessor asset's whole state to the next step
(`interpreter::do_step_state`), so in `titled/retitled` the `retitled` command *receives* `T1`/`D`
in `state.metadata`. The final asset's own record is unchanged — still `T2`/empty — so the
question this issue asks (inherit into the final asset, or keep per-asset semantics) is still
open. The inheritance design's `Step::Evaluate` change now belongs in `do_step_state`.

## Expected behaviour

Decide whether a predecessor's title/description should be inherited by its consumer when the
consumer sets none, or whether the current per-asset semantics is the intended contract and
should stay documented as such.
