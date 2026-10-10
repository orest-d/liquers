# Phase 4: Implementation Plan — plan-step-state-metadata

## Overview

Four code steps in dependency order, then the `liquers-lib` proof. `bin` goes first because it is
independent and later tests compare against an absent format. The `Step::Action.query` field comes
before the interpreter that reads it. The cut rule comes before the interpreter so that removing
`value_origin_key` never runs against a plan that still wraps a bare key read in a boundary.
No prerequisite issues (Phase 2 preflight). Signatures re-opened at `b516426`:
`Recipe::data_format` / `get_asset_info` (`recipes.rs` ≈212, ≈403), `Step::Action` (`plan.rs`
≈268), `PlanBuilder::process_action` (≈1590-1690) and its single-transform caller (≈1770),
`Plan::freeze_cwd_with` (≈2108), `Plan::cut_predecessor` (≈2198), `apply_plan` / `do_step` /
`value_origin_key` / `fetched_key` (`interpreter.rs` ≈331-470, ≈569), `resolve_absolute_query_resource_step`
(≈293), `AssetRef::set_description_fields_from_command` (`assets.rs` ≈4215).

## Progress

- [ ] Step 1: `Recipe` declares no format without a filename
- [ ] Step 2: `Step::Action` records its prefix query
- [ ] Step 3: the cut declines a bare key read
- [ ] Step 4: each step builds its next state; `value_origin_key` removed
- [ ] Step 5: `liquers-lib` proof and full validation

## Implementation Steps

### Step 1: `Recipe` declares no format without a filename
- Files / symbols: `liquers-core/src/recipes.rs` `Recipe::data_format`, `Recipe::get_asset_info`;
  `liquers-lib/src/egui/widgets.rs` (recipe widget, ≈61).
- Change: `pub fn data_format(&self) -> Result<Option<String>, Error>` returning
  `self.extension()`; `get_asset_info` assigns it unchanged; the widget shows the row on `Some`.
  Update unit tests in `recipes.rs` / `assets.rs` that assert `bin`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib recipes` (tests 20, 21);
  `cargo check -p liquers-lib --features egui`.
- Rollback: revert the commit; nothing else depends on it until Step 4's tests.

### Step 2: `Step::Action` records its prefix query
- Files / symbols: `liquers-core/src/plan.rs` `Step::Action`, `PlanBuilder::process_action`, the
  single-transform branch of the query walk, `Plan::freeze_cwd_with`;
  `liquers-core/src/recipes.rs` `Recipe::to_plan`; `liquers-core/src/interpreter.rs`
  `resolve_absolute_query_resource_step`; literal `Step::Action { … }` constructions in
  `plan.rs`/`interpreter.rs` tests, `liquers-core/tests/command_alias.rs`,
  `liquers-core/tests/validate_integration.rs`, `liquers-lib/tests/plan_namespace_resolution.rs`.
- Change: the field from Phase 2 §Interfaces. `process_action` takes the prefix as an argument
  (the single-transform caller passes the unemptied query) and stores
  `promote_relative_default_links(prefix, cmr)`. `freeze_cwd_with` resolves each action's query
  with the post-prologue cursor used for `predecessor`. `to_plan` clears every action's query when
  `has_arguments()`. Patterns with `..` need no change.
- Depends on: none (parallel with Step 1).
- Proof: `cargo test -p liquers-core --lib plan` (tests 15-18);
  `cargo test -p liquers-core --test command_alias --test validate_integration --test plan_cwd_freeze`;
  `cargo check -p liquers-lib --tests`.
- Rollback: revert; the field is optional and unread until Step 4.

### Step 3: the cut declines a bare key read
- Files / symbols: `liquers-core/src/plan.rs` `Plan::cut_predecessor`.
- Change: after the walk-back, return `Ok(false)` with
  `init_info("Predecessor boundary not cut: the prefix only reads a key")` when
  `steps[prologue_steps..cut_at]`, ignoring `SetCwd`, is one key-read step (Phase 1 Decision 4;
  the match enumerates every `Step` variant). Update tests that assert the boundary shape of
  `-R/<key>/-/ns-x/action`.
- Depends on: none; must land before Step 4 removes `fetched_key`.
- Proof: `cargo test -p liquers-core --lib plan` (test 19);
  `cargo test -p liquers-core --test plan_cwd_freeze --test recipe_cwd_resolution`.
- Rollback: revert; `fetched_key` still covers the boundary until Step 4.

### Step 4: each step builds its next state; `value_origin_key` removed
- Files / symbols: `liquers-core/src/interpreter.rs` `apply_plan`, `do_step` (become wrappers),
  new `apply_plan_state`, `do_step_state`, `prefix_metadata`; delete `value_origin_key`,
  `fetched_key`, `fetched_key_honours_the_resource_header`; `liquers-core/src/assets.rs` new
  `AssetRef::recipe_declared_description`; `liquers-lib/src/records/commands.rs` ≈575 comment.
- Change: the next-state table of Phase 2, enumerated over every `Step` variant; `GetAsset` sets
  `key` when the fetched record lacks it; `GetResource` keeps the stored metadata.
- Depends on: Steps 1-3.
- Proof: `cargo test -p liquers-core --test plan_step_state_metadata` (tests 3-14);
  `cargo test -p liquers-core --lib --tests`.
- Rollback: revert this step alone; Steps 1-3 stand on their own (Step 3 then only removes a
  pointless boundary, and the record-streams key falls back to the `GetAsset` path, which
  `value_origin_key` also covers).

### Step 5: `liquers-lib` proof and full validation
- Files / symbols: `liquers-lib/tests/polars_commands.rs` (tests 22, 23),
  `liquers-lib/tests/record_manifest_resource_key.rs` (test 24); any `liquers-lib` test asserting
  `bin` or the old step state.
- Change: tests only.
- Depends on: Step 4.
- Proof: `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-lib --no-default-features --features records --lib --tests`;
  `bash scripts/check-build-matrix.sh`.
- Rollback: tests only.

## Testing Plan

Each step's proof runs when the step lands (`CARGO_INCREMENTAL=0`). After Step 5: the two `cargo
test` loops above for `liquers-core` and `liquers-lib`, and `scripts/check-build-matrix.sh`
because Step 1 touches an `egui`-gated caller. `specs/command_registry.yaml` is unaffected (no
command signature or body changes), so `registry_export` must stay green unchanged.

## Rollback Plan

The whole change is five commits on one branch; revert in reverse order. Steps 1-3 are each
independently correct. Only Step 4 depends on all three; reverting it alone restores today's step
states with the new plan field unread.

## Documentation Updates

Phase 5, against the implemented behaviour:
- `specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`: new §Metadata ownership during
  evaluation (moving Phase 1's Background there and trimming Phase 1); §Context lifetime rewritten.
- `specs/reference/api/DOC_08_RECIPES_PLANS.md`: `Step::Action.query`; next-state table; cut
  exception; recipe `data_format`.
- `specs/reference/VALUE_TYPE_SYSTEM.md`: checked; History row if touched.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md`: input description vs asset record.
- `Context::get_metadata` doc comment (with Step 4).
- Each changed reference/guide: `## History` row and `reviewed:` bump (§9.2).
- `specs/index.csv` / `index.md` regenerated; `specs/README.md` capability line.
- Issues: `CONTEXT-STEP-REPLACES-INPUT-STATE-METADATA` and
  `FREE-FUNCTION-EVALUATE-BAKES-A-BIN-DATA-FORMAT` closed with resolution notes; note in
  `CORE-PLAN-POLICY-AND-DEFAULTS` the new cut policy.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
