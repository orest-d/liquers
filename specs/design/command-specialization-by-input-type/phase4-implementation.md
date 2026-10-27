# Phase 4: Implementation Plan — a generic command dispatches by its input's type

## Overview

Prerequisite: `command-io-type-declarations` steps 1-3 (for `head`, also its step 5). Then the
registry model (1-2), the planner (3), the step and interpreter (4), the shipped `head` (5),
bindings and export (6), documents (7).

## Progress

- [ ] Step 1: `CommandDefinition::Generic`, `generic_variants`, `select_variant`, error constructor
- [ ] Step 2: `register_generic`
- [ ] Step 3: `ActionOrigin::Specialized` and plan-time specialization
- [ ] Step 4: `Step::Dispatch` and run-time selection
- [ ] Step 5: the `head` generic in `liquers-lib`
- [ ] Step 6: bindings and registry export
- [ ] Step 7: documentation and final checks

## Implementation Steps

### Step 1: `CommandDefinition::Generic`, `generic_variants`, `select_variant`, error constructor
- Files / symbols: `liquers-core/src/command_metadata.rs` — enum arm; `alias_target` refuses a
  `Generic` target (explicit arm); `CommandMetadataRegistry::generic_variants`; free function
  `select_variant`. `liquers-core/src/error.rs` — `Error::no_command_variant`. `commands.rs` —
  `register_alias`'s `head_count` match gets a `Generic` arm (0; unreachable after `alias_target`).
- Depends on: `command-io-type-declarations` step 2.
- Proof: `cargo test -p liquers-core --lib command_metadata` — test 9 and unit tests of
  `select_variant` precedence.
- Rollback: revert; nothing produces the arm yet.

### Step 2: `register_generic`
- Files / symbols: `liquers-core/src/commands.rs` — `CommandRegistry::register_generic` per Phase 2
  (checks via `generic_variants` on a candidate metadata before storing; `unregister` then
  `add_command`, as `register_alias`).
- Depends on: 1.
- Proof: `cargo test -p liquers-core --test generic_commands register_generic_refuses_bad_variants`.
- Rollback: revert.

### Step 3: `ActionOrigin::Specialized` and plan-time specialization
- Files / symbols: `liquers-core/src/plan.rs` — `ActionOrigin::Specialized`, `alias()`, `named()`;
  extract `PlanBuilder::action_step` from the two existing arms of `process_action` (behaviour-neutral
  refactor, committed alone); the `Generic` arm; `scan_plan` and `last_action_command` arms;
  `interpreter.rs` `requires_payload` and the error-naming `match origin.alias()` use `named()`.
- Depends on: 1; `command-io-type-declarations` step 3 (`flowing`).
- Proof: refactor: `cargo test -p liquers-core --lib plan` unchanged; then test 4.
- Rollback: revert the arm; the refactor may stay.

### Step 4: `Step::Dispatch` and run-time selection
- Files / symbols: `plan.rs` — `Step::Dispatch`, `DispatchVariant`, every exhaustive match listed
  in Phase 2 (no `_ =>`); the `Generic` arm emits it. `interpreter.rs` — `do_step`, `do_step_state`,
  `schedule_plan_dependencies_from`, `resolve_absolute_query_resource_step`, `log_init_steps`.
  `validate/report.rs`, `store/realm.rs`, `liquers-mcp/src/server.rs` — their arms.
- Depends on: 3.
- Proof: `cargo test -p liquers-core --test generic_commands` (tests 3, 5, 6, 8);
  `cargo test -p liquers-mcp`.
- Rollback: revert step 4 and the `Dispatch` branch of step 3's arm (plan-time only remains).

### Step 5: the `head` generic in `liquers-lib`
- Files / symbols: `liquers-lib/src/commands.rs` — `register_generic` for root `head` after the
  `ns-pl` and `ns-rec` registrations, gated on `any(polars, records)`, listing the variants built.
  New `liquers-lib/tests/generic_head.rs` (gated `polars` + `records`): tests 10-11.
- Depends on: 2, 4; `command-io-type-declarations` step 5.
- Proof: `cargo test -p liquers-lib --test generic_head`; `bash scripts/check-build-matrix.sh`.
- Rollback: remove the registration.

### Step 6: bindings and registry export
- Files / symbols: `liquers-py/src/command_metadata.rs` definition wrapper; regenerate
  `specs/command_registry.yaml` with `export-command-registry` and add a CHANGELOG line.
- Depends on: 5.
- Proof: `cargo test -p liquers-lib --test registry_export`; `bash scripts/test-liquers-py.sh`;
  rebuilt `liquers-validate -- '-R/data/sales.csv/-/head-5'` reports `Ok` (test 12).
- Rollback: regenerate from the reverted tree.

### Step 7: documentation and final checks
- Files / symbols: Phase 2's four documents with History and `reviewed:`; issue status per
  `DOCS_STRUCTURE_GUIDE.md` §4.3.
- Depends on: 1-6.
- Proof: `python3 scripts/docs_index.py --check`; final diff review.

## Testing Plan

Focused per step as above. Crates: `cargo test -p liquers-core`, `cargo test -p liquers-lib --lib
--tests`, `cargo test -p liquers-mcp`. Matrix and bindings: `check-build-matrix.sh`,
`test-liquers-py.sh`. Every AC-1…AC-8 is cited by a Phase 3 test.

## Rollback Plan

Steps revert in reverse order; step 5 alone removes the user-visible command. No persisted data
depends on the feature except serialized plans that contain a `Dispatch` step or a `Specialized`
origin, which an older build refuses to read rather than misreads.

## Documentation Updates

`COMMAND_ALIASES.md`, `RECIPES_AND_PLANS.md`, `COMMAND_DECLARATION.md`,
`COMMAND_REGISTRATION_GUIDE.md` (step 7); `specs/command_registry.yaml` (step 6).

## Phase 5 Entry Criteria

Not applicable (no `workflow` marker). Done when the Phase 1 decisions are recorded, every proof
passes and `docs_index.py --check` is clean.
