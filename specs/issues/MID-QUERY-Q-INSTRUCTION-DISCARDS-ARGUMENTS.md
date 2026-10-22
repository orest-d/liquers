---
id: MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS
kind: issue
title: A q instruction in the middle of a query silently drops its parameters
status: draft
priority: P3
complexity: S
area: [core/plan]
design:
created: 2026-10-20
github:
---
## Problem

`PlanBuilder::process_query` checks the arity of `q` only when `q` ends the query
(`liquers-core/src/plan.rs`, the `query.is_q()` branch: "The 'q' instruction does not accept any
arguments"). When `q` is in the middle of a query, the predecessor branch (`if p.is_q()`) strips
it with `strip_q_instruction` and never looks at its parameters. `liquers-validate -- 'nosuch/q-x/upper'`
(with `upper` registered) plans cleanly; `nosuch/q-x` is `NotSupported`.

## Impact

A typo or a misunderstanding of `q` (`a/q-raw/b`) is accepted and the parameter is silently
discarded, contrary to the arity rule every other action and instruction follows
(`PLAN-EXCESS-ACTION-PARAMETERS-DROPPED`). Low impact: no known query relies on it.

## Expected behaviour

`q` with a parameter is rejected with the same positioned `NotSupported` error wherever it
appears, and `special_instructions_enforce_their_own_arity` covers the mid-query form.

## Discovery

Found while reviewing `reference/api/DOC_08_RECIPES_PLANS.md` against the code at `96279f14` before promoting it to [`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md) (2026-10-20).
