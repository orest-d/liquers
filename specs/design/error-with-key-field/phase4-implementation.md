# Phase 4: Implementation Plan — Structured Error Context

Written 2026-10-20 under Phase 1's recommended answers; do not start before they are decided
(readiness `needs-decision`).

## Overview

Core model first, then the flat-field fix with the cancellation contract that depends on it, then
the producers, then the consumers one crate at a time. Each step leaves the workspace building and
its tests green. Prerequisites: none (`key-without-recipe-answer` may land before or after).

## Progress

- [ ] Step 1: frame types, `context` field and builders
- [ ] Step 2: `with_key` and the keyed constructors; cancellation contract
- [ ] Step 3: asset frame on every run failure
- [ ] Step 4: link and action boundaries
- [ ] Step 5: resource frames in the store constructors
- [ ] Step 6: `LogEntry::from_error`
- [ ] Step 7: HTTP, Python, JavaScript, egui consumers
- [ ] Step 8: documents and records

## Implementation Steps

### Step 1: frame types, `context` field and builders
- Files / symbols: `liquers-core/src/error.rs` — `ErrorContextRole`, `ErrorContextFrame`,
  `ErrorPayload::context`, `ErrorPayload::new`, `MAX_ERROR_CONTEXT_FRAMES`, `with_context`,
  `with_asset_context`, `with_link_context`, `with_action_context`, `ErrorContextFrame::{asset, resource}`.
- Change: Phase 2 Interfaces; re-export from `liquers_core::error`.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib error::tests` — tests 6, 7.
- Rollback: revert; nothing uses the field yet.

### Step 2: `with_key` and the keyed constructors; cancellation contract
- Files / symbols: `error.rs` `with_key`, `key_not_found`, `status_conflict`,
  `dependency_version_mismatch`, `dependency_cycle`, `ErrorType::Cancelled` doc; `assets.rs`
  `AssetRef::attribute_run_error` (`query.is_none() && key.is_none()`),
  `cancelled_dependency_cause` (root from `key`, then `query`); tests in `assets.rs`
  (cancellation cascade), `plan.rs` (`walk_detects_recipe_cycle`), `tests/asset_cancellation.rs`.
- Change: AC-1, AC-2, AC-7.
- Depends on: none (independent of step 1).
- Proof: `cargo test -p liquers-core --lib error::tests` (tests 3, 4) and
  `cargo test -p liquers-core --lib --tests` (cancellation suite green with updated asserts).
- Rollback: revert as one commit; the contract and its readers move together.

### Step 3: asset frame on every run failure
- Files / symbols: `assets.rs` `AssetRef::attribute_run_error` — append
  `with_asset_context(key, query)` from `AssetData` for every error (cancellations included, after
  attribution).
- Depends on: steps 1, 2.
- Proof: `cargo test -p liquers-core --lib assets::tests::query_asset_failure_names_its_query`,
  `…::failed_keyed_asset_keeps_an_inner_key`; then the full core suite (asserts on `error_data`
  equality may need the frame added).
- Rollback: revert step 3; the field stays empty.

### Step 4: link and action boundaries
- Files / symbols: `interpreter.rs` `materialize_link_json` (`attach_context`),
  `attach_parameter_link_context`, the `ce.execute_async(...).map_err` in `do_step` `Step::Action`.
- Change: `with_link_context`; `with_action_context` when `command_key` is already set.
- Depends on: step 1.
- Proof: `cargo test -p liquers-core --lib interpreter::tests::nested_link_failure_records_the_path`,
  `…::outer_action_does_not_overwrite_inner_position`; full core suite (position assertions in
  existing interpreter tests must still hold for single-level failures).
- Rollback: revert step 4.

### Step 5: resource frames in the store constructors
- Files / symbols: `error.rs` `key_read_error`, `key_write_error`, `key_not_supported`.
- Depends on: step 1.
- Proof: `cargo test -p liquers-core --lib error::tests::store_constructors_add_a_resource_frame`;
  `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF` (rules
  compare error types, not payloads).
- Rollback: revert step 5.

### Step 6: `LogEntry::from_error`
- Files / symbols: `liquers-core/src/metadata.rs` `LogEntry::from_error`.
- Depends on: step 2.
- Proof: `cargo test -p liquers-core --lib metadata::tests::log_entry_from_key_only_error_has_a_query`.
- Rollback: revert.

### Step 7: HTTP, Python, JavaScript, egui consumers (parallel)
- Files / symbols: `liquers-axum/src/api_core/{response,error}.rs` (`ErrorDetail.context`,
  `error_to_detail`); `liquers-py/src/error.rs` (`error_to_pyerr`, `error_from_pyerr`) and
  `python/liquers/_errors.py` (`_from_native`, `LiquersError.__init__`); `liquers-web/src/error.rs`
  (`context` getter) and its stub; `liquers-lib/src/egui/widgets.rs` (key fallback, frame lines).
- Depends on: steps 1-5.
- Proof: `cargo test -p liquers-axum`; `bash scripts/test-liquers-py.sh` (test 15);
  after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
  (test 16) and `./liquers-web/scripts/check-stubs.sh --build`; `cargo test -p liquers-lib --lib --tests`.
- Rollback: each crate's change reverts alone.

### Step 8: documents and records
- Files: Phase 2 Documentation Architecture; `specs/README.md` link; `affects_docs` in `DESIGN.md`
  updated to the Phase 2 set; `ERROR-WITH-KEY-SETS-QUERY-FIELD` → `closed` with resolution;
  note on `CORE-ERROR-STORE-NAME-NOT-STRUCTURED`'s successor; Phase 5 per `workflow: liquers-project`.
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

Steps 1-6: `cargo test -p liquers-core --lib --tests` after each. Step 7: each consumer crate's
suite, then `cargo test -p liquers-lib --lib --tests`. Final: `bash scripts/check-build-matrix.sh`
(the egui widget is `cfg(feature = "egui")`; wasm32 rows build `liquers-core` with the new types).

## Agent Assignment

One implementer for steps 1-6 (all `liquers-core`, sequential); step 7 can be split per consumer
crate (axum, py, web, egui) once steps 1-5 are merged; step 8 by the implementer of step 1.

## Rollback Plan

Revert in reverse order. If only steps 1-2 land, keys are correct and the field is empty — a
coherent state. If step 7 is partial, the unchanged bindings simply do not expose `context`.

## Documentation Updates

`specs/reference/ERROR_CONTEXT.md` (new), the reference and guide updates of Phase 2, each with a
History row and `reviewed:` bump; `specs/README.md`; `python3 scripts/docs_index.py`. No
`specs/command_registry.yaml` change.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
