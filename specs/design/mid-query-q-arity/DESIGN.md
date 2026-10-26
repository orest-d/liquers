---
id: MID-QUERY-Q-ARITY
kind: design
title: A q instruction with a parameter is refused wherever it appears in a query
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/plan]
issues: [MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS]
created: 2026-10-26
---
# A q instruction with a parameter is refused wherever it appears in a query

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

`q` takes no parameters. A trailing `q-x` is refused, but a `q-x` followed by more actions is
accepted and its parameter is dropped. Every other action and instruction refuses surplus
parameters (`excess-action-parameters-error`), so `q` should follow the same rule in every position.

### Problem Example

Verified at HEAD (cd6d5da) with `liquers-validate --command upper`:

| Query | Today | Should be |
|---|---|---|
| `nosuch/q-x` | `Error`: "The 'q' instruction does not accept any arguments" | unchanged |
| `nosuch/q-x/upper` | `Ok`, `x` is silently discarded | the same `NotSupported` error, positioned at `x` |
| `nosuch/q/upper` | `Ok` | unchanged |

Cause: `PlanBuilder::process_query` (`liquers-core/src/plan.rs`) checks the arity in the
`query.is_q()` branch, which handles a query that *ends* in `q`. A `q` in the middle is reached
through the predecessor branch (`if p.is_q()`), which calls `strip_q_instruction(p)` and never
looks at the parameters.

### Scope and Acceptance Criteria

- **AC-1** Mid-query `q` with a parameter is refused
  WHEN `nosuch/q-x/upper` is planned (with `upper` registered)
  THEN planning fails with `ErrorType::NotSupported`, message "The 'q' instruction does not accept
  any arguments", positioned at the parameter `x` (offset 9)
- **AC-2** Deeper in a chain
  WHEN `a/q-x/b/c` is planned (with `b` and `c` registered)
  THEN the same error is returned
- **AC-3** Trailing `q` unchanged
  WHEN `a/q-extra` is planned
  THEN the existing error and position are returned, as `special_instructions_enforce_their_own_arity` pins
- **AC-4** Valid `q` unchanged
  WHEN `nosuch/q/upper` or `-R/data/test.csv/-/command1-arg/q/command2-arg` is planned
  THEN the plans are as today (`UseQueryValue` then the action; `test_q_instruction_plan` stays green)

Non-goals: planning or validating the quoted part (it stays unplanned by design,
`RECIPES_AND_PLANS.md` §Planner instructions); the `ns` and `v` arity rules (already enforced).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring the documented arity rule, in
  `liquers-core/src/plan.rs` only: one private helper and two call sites. Queries that plan today
  and carry a parameter on `q` start to fail, which is the documented behaviour and not an interface
  change (rule 4). No `pub` item, command or format changes.
- **Leading issue:** None
- **Explanation:** the existing trailing check is moved into a helper that both branches call. The
  error type, message and position are those already used for a trailing `q`.
- **Open questions:** None.

### Design Dependencies

- `reserved-instruction-command-names` — **overlaps** (finished Phase 4, not edited). It changes
  `ActionRequest::is_q` to compare against a constant, which this design calls but does not change.
  No order is required.
- `planner-instruction-names-public` — **overlaps**: makes the instruction names public. It does not
  touch `process_query`'s branches. No order.

### Consolidated Findings

- The check belongs in `strip_q_instruction`'s callers, not in `strip_q_instruction` itself, which
  returns a `Query` and has no error path. A private `check_q_arity(&Query) -> Result<(), Error>`
  keeps the message and position identical in both branches.
- Error type: `NotSupported` (today's trailing behaviour), not `TooManyParameters` (which `v` uses).
  Unifying the two types would change an observable error type for the trailing case, so it is out
  of scope.
- `RECIPES_AND_PLANS.md` §Planner instructions and its Known limitations list name this issue; both
  sentences change with the fix.

## Phase 2: Architecture

### Solution

In `liquers-core/src/plan.rs`:

1. Add a private method beside `strip_q_instruction`:

   ```rust
   /// Refuses a `q` that ends `query` and carries a parameter; `q` takes none.
   fn check_q_arity(query: &Query) -> Result<(), Error>
   ```

   The body is the current inline check from the `query.is_q()` branch: last transform segment →
   last action → `if is_q() && parameters.first()` is some → `Err(Error::not_supported("The 'q'
   instruction does not accept any arguments".to_string()).with_position(&excess.position()))`.
2. `process_query`, `query.is_q()` branch: replace the inline block with `check_q_arity(query)?`.
3. `process_query`, predecessor branch, `if p.is_q()`: call `check_q_arity(p)?` before
   `strip_q_instruction(p)`.

Every mid-query `q` reaches the predecessor branch at some recursion level, because
`Query::predecessor` peels one action at a time and `p.is_q()` holds exactly when `q` is the last
action of the predecessor. So two call sites cover all positions.

Rejected: checking every action of the whole query for `q` up front (duplicates the recursion and
would also inspect the unplanned quoted part); making `strip_q_instruction` fallible (changes
its role and both call sites more).

### Changes

- `liquers-core/src/plan.rs`: `check_q_arity` (new, private), `process_query` (two call sites),
  tests.
- Documents: `specs/reference/RECIPES_AND_PLANS.md` — §Planner instructions, `q` paragraph ("a `q`
  in the middle of a query currently ignores its parameters" becomes "a `q` with a parameter is
  `NotSupported` wherever it appears"); remove the Known limitations bullet; History row; bump
  `reviewed:`.

Sync only. No `pub` change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `plan.rs`, `RECIPES_AND_PLANS.md` |
| Existing tests | `test_q_instruction_*` and `special_instructions_enforce_their_own_arity` stay green; none uses `q-<param>` mid-query |
| Compatibility | a stored recipe whose query has `…/q-x/…` now fails to plan. None is known: `grep -rn 'q-' specs/command_registry.yaml` and the repository's recipes show none |
| Performance | one extra branch per predecessor that ends in `q` |
| Recovery | revert |
| Certainty | high |

## Phase 3: Examples and Tests

### Examples

Validated with `liquers-validate --command upper` at HEAD: `nosuch/q-x/upper` → `Ok` today
(encoded `nosuch/q-x/upper`), `nosuch/q-x` → `Error`, `nosuch/q/upper` → `Ok`. After the fix, the
first is `Error` with the second's message.

### Tests

In `liquers-core/src/plan.rs` `mod tests`:

- Extend `special_instructions_enforce_their_own_arity` (AC-1, AC-3): after the trailing `a/q-extra`
  assertion, `plan_for("nosuch/q-x/a", &cr)` is `Err` with `NotSupported`, message containing "does
  not accept any arguments", and `err.position.offset == 9`. `nosuch` is unregistered on purpose:
  the quoted part is not planned.
- `mid_query_q_with_parameter_deep_in_chain_is_refused` (AC-2): registry with `b`, `c`;
  `plan_for("a/q-x/b/c", &cr)` is `Err(NotSupported)`.
- Existing `test_q_instruction_plan` and the other `test_q_instruction_*` tests (AC-4), plus a new
  assertion that `plan_for("nosuch/q/a", &cr)` is `Ok` with steps `[UseQueryValue(nosuch), Action(a)]`.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib plan::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `plan.rs` tests — extend `special_instructions_enforce_their_own_arity`, add `mid_query_q_with_parameter_deep_in_chain_is_refused` and the `nosuch/q/a` assertion; the two refusals fail at HEAD — `cargo test -p liquers-core --lib special_instructions mid_query_q`
- [ ] 2. `plan.rs` — add `check_q_arity`; use it in the `query.is_q()` branch and before `strip_q_instruction(p)` in the predecessor branch — depends on 1 — step 1 tests pass; `cargo test -p liquers-core --lib plan::tests`. Rollback: revert.
- [ ] 3. `specs/reference/RECIPES_AND_PLANS.md` — `q` paragraph and Known limitations; History row, `reviewed:` — `python3 scripts/docs_index.py --check`
- [ ] 4. Close `MID-QUERY-Q-INSTRUCTION-DISCARDS-ARGUMENTS` with a resolution note; this design to `complete` — `python3 scripts/docs_index.py` then `--check`

### Validation

`cargo fmt --check -p liquers-core`; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`;
`./target/debug/liquers-validate --command upper -- 'nosuch/q-x/upper'` now reports the error
(after rebuilding the validator). Diff review: `plan.rs` and the reference document only.
