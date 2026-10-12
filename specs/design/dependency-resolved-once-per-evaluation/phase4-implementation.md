# Phase 4: Implementation Plan — dependency-resolved-once-per-evaluation

## Overview

Build in three separable groups, in this order:

1. **Session and pinning** (steps 2-6). This fixes the P1 issue on the queued manager and is
   worth shipping on its own.
2. **Inline contract changes and fail-fast** (step 7).
3. **Stack depth** (steps 8-10): the queued spawn, then inline priming.

Tests come first (step 1), so each later step turns named tests green. No prerequisite issue must
be fixed first (Phase 2 preflight). Everything is in `liquers-core`, plus documents.

## Progress

- [ ] Step 1: Phase 3 integration tests written; HEAD failures recorded
- [ ] Step 2: `evaluation_session.rs` module and unit tests
- [ ] Step 3: session field on `AssetData` / `AssetRef`, carried by `Context`
- [ ] Step 4: pinning in `Context::schedule_dependency_asset_with_key`
- [ ] Step 5: queued manager adopts the session; `apply_for`
- [ ] Step 6: AC-4 assertion tightened
- [ ] Step 7: inline manager returns failed assets; default `wait_for_dependency` `Error` arm; pre-pass fail-fast
- [ ] Step 8: queued spawn of locally queued runs
- [ ] Step 9: `plan.rs` `upstream_graph`
- [ ] Step 10: inline priming
- [ ] Step 11: full regression (native, wasm)
- [ ] Step 12: documentation
- [ ] Step 13: records

## Implementation Steps

### Step 1: Tests first
- Files: new `liquers-core/tests/evaluation_session.rs`, new `liquers-core/tests/deep_chain_evaluation.rs`
  (the Phase 3 lists).
- Proof: `cargo test -p liquers-core --test evaluation_session --test deep_chain_evaluation`
  compiles. The failing tests are recorded here, with deep tests run one by name because an abort
  stops the binary. Gated tests use latches and run under `tokio::time::timeout`, so they fail
  rather than hang at HEAD. Expected to fail at HEAD: AC-1 to AC-3, AC-6 to AC-10, AC-13 (the gated
  command enters twice), AC-14. Expected to pass: AC-5, AC-11 (probably), AC-12, AC-15, AC-16.
- Rollback: delete the two files.

### Step 2: `EvaluationSession`
- Files: new `liquers-core/src/evaluation_session.rs`; `lib.rs` `pub(crate) mod evaluation_session;`.
- Change: Phase 2 interfaces; unit tests `resolve_runs_the_resolver_once`,
  `resolve_error_pins_nothing`, `pin_keeps_the_first_asset`.
- Depends on: none. Proof: `cargo test -p liquers-core --lib evaluation_session`.
- Rollback: remove the module.

### Step 3: Carry the session
- Files: `assets.rs` `AssetData` (field `evaluation_session: Weak<…>`, set in every constructor),
  `AssetRef::{evaluation_session, adopt_evaluation_session, evaluation_session_or_begin}`;
  `context.rs` `Context` field, `Context::new`, and the literals in `with_volatile`,
  `clone_context`, `Clone::clone` and the test constructor; the module doc ("no user session").
- Depends on: 2. Proof: `cargo test -p liquers-core --lib` (no behaviour change yet) and unit test
  `assets::tests::evaluation_session_ends_with_its_last_run`.
- Rollback: drop the field and methods.

### Step 4: Pin in `Context`
- Files: `context.rs` `schedule_dependency_asset_with_key`: `self.evaluation_session.resolve(...)`
  around `get_dependency_asset`, after `register_scheduled_dependency`.
- Depends on: 3. Proof: `cargo test -p liquers-core --test evaluation_session failing_chain`,
  `command_reading_dependency_twice`, `failure_reruns`. AC-2 may still need step 5.
- Rollback: call the manager directly again.

### Step 5: Queued adoption and `apply_for`
- Files: `assets.rs` `DefaultAssetManager::get_dependency_asset` (adopt before
  `try_to_start_immediately` / `push_local_dependency`); the doc comment of the trait's
  `get_dependency_asset` (return failed assets, adopt the session); `AssetManager::apply_for` (provided);
  `DefaultAssetManager::apply_for`; `context.rs` `Context::apply` → `apply_for`.
- Depends on: 4. Proof: `cargo test -p liquers-core --test evaluation_session` (queued tests:
  AC-1, AC-2, AC-3, AC-5 to AC-8).
- Rollback: remove the adoption calls; `apply_for` falls back to `apply`.

### Step 6: AC-4
- Files: `tests/dependency_audit_integration.rs` `restart_upstream_command_change_on_load_refuses`
  (`assert_eq!(…, 2)`, drop the issue comment); the same for
  `restart_deep_expired_status_on_load_refuses` if its count is now exact.
- Depends on: 5. Proof: `cargo test -p liquers-core --test dependency_audit_integration`.
- Rollback: restore `>=`.

### Step 7: Inline contract and fail-fast
- Files:
  - `assets.rs` trait-default `AssetManager::wait_for_dependency`: an `Error` arm returning
    `stored_error()` (fallback message as in `DefaultAssetManager`).
  - `ImmediateAssetManager`: `settle`, `settle_query`; `get_dependency_asset` (both branches)
    settles, adopts, runs, and returns a failed asset; `apply_for`.
  - `interpreter.rs` `schedule_plan_dependencies_from`: the early exit for an already-failed
    dependency.
- Depends on: 5 (`apply_for`). Proof: `cargo test -p liquers-core --test evaluation_session inline`
  (AC-7 inline, AC-15), `dynamic_cycle` (AC-16), and `--test manager_parametric --test
  asset_failure_contract --test payload_inheritance --test external_asset_manager` (the trait default
  is what `MinimalInlineAssetManager` uses).
- Rollback: revert the three files' hunks together (they are one contract).

### Step 8: Queued spawn
- Files: `assets.rs` `DefaultAssetManager::drain_dependencies` and the direct-claim branch of
  `wait_for_dependency`: spawn run + `claim.complete()` + re-park of the child's undrained local
  queue; await the handle.
- Depends on: none (parallel with 4-7). Proof:
  `cargo test -p liquers-core --test deep_chain_evaluation queued` (AC-9, AC-13: the gated command
  enters once) and
  `--test asset_cancellation --test dependency_scheduling`.
- Rollback: restore `Box::pin(dep.run(None)).await`.

### Step 9: `upstream_graph`
- Files: `plan.rs` `DependencyWalk` recorder, `upstream_graph`, unit test
  `upstream_graph_is_post_order_without_volatile_keys`.
- Depends on: none. Proof: `cargo test -p liquers-core --lib upstream_graph`.
- Rollback: remove.

### Step 10: Inline priming
- Files: `assets.rs` `ImmediateAssetManager::prime_upstream`; the top-level `get`,
  `get_query_asset_run(TopLevel)`, `apply`, `apply_queued`. Priming starts only after `settle`
  finds the root neither finished nor fast-trackable, and skips `is_volatile(key)`.
- Depends on: 7, 9. Proof: `cargo test -p liquers-core --test deep_chain_evaluation immediate`
  (AC-10, AC-11, AC-14) and `cycle` (AC-12).
- Rollback: drop the priming calls (the session still works without them).

### Step 11: Regression
- Proof:
  - `cargo test -p liquers-core --lib --tests`
  - `cargo test -p liquers-lib --lib --tests`
  - `cargo check -p liquers-core --all-features --lib --tests` (the `cli` rows of
    `check-build-matrix.sh`), `cargo check -p liquers-axum`, and `cargo check -p liquers-py` where a
    Python interpreter is available
  - `cargo check -p liquers-core --target wasm32-unknown-unknown`
  - after `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
  - `chain_20_links_smoke` within its bound.

  No `cfg(feature)` or optional dependency changes, so the build matrix is not required.

### Step 12: Documentation
- Files: the five Phase 2 documents, each with a `## History` row and a `reviewed:` bump.
- Proof: `python3 scripts/docs_index.py --check`.

### Step 13: Records
- `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (the residual limit) was filed at the Phase 4
  review; confirm that its text matches what was built.
- Update the link-by-link comment in `dependency_audit_integration.rs` `chain_first_process`.
- `specs/command_registry.yaml` is unaffected (no command changes).
- Proof: `python3 scripts/docs_index.py --check`.

## Testing Plan

- Steps 2-10: each step's named tests, plus `cargo test -p liquers-core --lib` after every step.
- Step 11: the full native core and lib loops, the wasm check and the Node loop.
- Deep tests run on explicit 1 MiB threads in their own binary.

## Rollback Plan

Each group reverts alone: inline priming (10), queued spawn (8), inline contract (7), then session
and pinning (2-5). If only group 1 lands, the P1 issue is fixed on the queued manager. The deep-chain
issue stays open, and AC-9 to AC-14 move to a follow-up issue (§5.6).

## Documentation Updates

`DEPENDENCIES_STATUS.md`, `ASSETS.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`,
`DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`, `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (Phase 2 table);
`specs/index.csv` and `specs/README.md` regenerated; History rows and `reviewed:` bumps.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
