# Phase 5: Documentation - dependency-resolved-once-per-evaluation

## Completion Preconditions

- [x] Implementation is finished and validated (see Validation)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (seven review passes over Phases 1-4)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation branch

## Implementation Summary

**What was built.** One evaluation is now one **evaluation session**
(`liquers-core/src/evaluation_session.rs`), and within it each dependency resolves to one asset.
The pieces:
- **Pinning:** `Context::schedule_dependency_asset_with_key` resolves every non-payload dependency
  request through the session. The first resolution is pinned, first-wins, with one `OnceCell` per
  dependency identity. Every later request gets the same asset, whatever its status.
- **Lifetime:** assets reference their session weakly (`AssetData::evaluation_session`), while
  running `Context`s hold it. A session therefore ends with its last run, and nothing pinned keeps
  it alive.
- **Hand-off:** `DefaultAssetManager::get_dependency_asset`, the inline manager's dependency path
  and both `apply_for` implementations give a dependency its parent's session before it starts.
- **Inline manager contract:** it settles before running (`settle`, `settle_query`) and returns a
  failed dependency as the asset. The trait-default `wait_for_dependency` turns an `Error`
  dependency into its stored error. The plan pre-pass stops at a dependency that has already
  failed.
- **Depth:** the queued manager runs each locally queued dependency in a task of its own
  (`run_claimed_in_own_task`). The inline manager primes a cold top-level asset's static upstream
  bottom-up (`prime_upstream`, over `plan.rs` `upstream_graph` / `key_upstream_graph`).

**Results** (Phase 1 examples):
- A failing 10-link chain runs `fail` once (was 240), at capacity 4 and at capacity 1 (was 1024).
- A failing lattice of depth 10 runs its root once (depth 4 did not finish at HEAD).
- A 20-link failing chain finishes at once.
- A volatile dependency has one value per evaluation (`tick` ran 4 times at HEAD).
- 100-link cold chains evaluate on a 1 MiB stack on both managers, the inline one with no runtime
  (HEAD overflowed at 27 and 10 links on 2 MiB).

**Conformance to the approved design.**
- **Two additions:**
  - `key_upstream_graph`, beside `upstream_graph`, primes a keyed top-level request without first
    building a plan for it.
  - `EvaluationSession` is built on a private `PinMap<T>`, so its first-wins logic is unit-tested
    without assets.
- **One omission:** `ImmediateAssetManager::apply_queued` is not primed. On a priming failure,
  `apply_queued` must return its asset rather than an error, and the ad-hoc asset can record the
  failure only by running. Running it then nests one level per link back down to the failure.
  Ad-hoc queued applies of deep cold chains are left to
  `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`, which already covers inline nesting that
  priming does not reach.
- **One correction:** Phase 2 asked managers to adopt the session. `adopt_evaluation_session` is
  crate-private, so only the built-in managers can, and the trait documentation says so. The gap is
  filed as `EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION`, because closing it widens the
  public API, which this design kept unchanged.

## Documentation Delivered

### New Reference Documents
None. The session is a contract of the existing dependency model, documented where that model is.

### New Guide Documents
None. Manager authors read `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`, which was extended.

### Existing Documents Reviewed or Updated
`affects_docs`, each with a `## History` row and `reviewed: 2026-10-12`:
- **`reference/DEPENDENCIES_STATUS.md`:**
  - new §Evaluation session (rules 1-6, the pre-pass early exit, depth);
  - the F-1 bullet, Flow A steps 5 and 7, Flow D item 2 and §Non-blocking dependency scheduling
    no longer say "inline".
- **`reference/ASSETS.md`:** §Re-evaluation qualified by the session.
- **`reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`:** the manager cache-miss text qualified
  by the session.
- **`reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`:**
  - the Context-methods table (`evaluate`, `apply`);
  - a new paragraph "One asset per dependency per evaluation".
- **`guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`:**
  - §4: what the session asks of `get_dependency_asset`, and `apply_for`;
  - §11: three new known limits.

### Links and Capability Map
`specs/README.md`: the map line "Evaluation session: each dependency evaluated once per
evaluation, stack depth independent of chain length" points at this design and moves to *built*.

## Issues Filed

- `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` (P3): the residual limit. On the inline manager,
  the part of a chain upstream of a dynamic or volatile dependency, and ad-hoc `apply_queued`,
  still nest one level per link.
- `EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION` (P3): an external manager cannot give a
  dependency its parent's session, so a shared upstream may run once per dependency.

Closed by this work:
- `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY`;
- `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-DURING-A-DEPENDENT-EVALUATION`;
- `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK`.

`deep-chain-evaluation-stack-depth` stays `superseded`.

## Important Learning

- **The exponential re-run came from two resolutions in one run.** The plan pre-pass and the step
  that reads a dependency each resolved it, and the manager's refresh of a stale `Error` turned the
  second into a re-run. Deduplicating inside the pre-pass could not see it.
- **The identity scope has to be the whole evaluation.** A memo per asset run bounded the
  blow-up but left 2-3 runs on a diamond, because sibling dependents are separate runs. That made
  "one asset per dependency per evaluation" the contract, and it settles volatile dependencies and
  repeated command requests too.
- **Lifetime without cycles.** Weak on the asset, strong in running contexts: a session dies with
  its last run, which the unit test `evaluation_session_ends_with_its_last_run` checks.
- **A hidden inconsistency between the managers:** the trait-default `wait_for_dependency` (used by
  the inline manager) handed an `Error` dependency on as an `Ok` error-state. It was invisible
  while the inline manager returned `Err` from `get_dependency_asset`.
- **Volatility is a recipe flag.** A volatile command inside a keyed recipe does not make the key
  volatile.
- **A recipe cycle never reaches evaluation;** the plan walk rejects it. Only a dynamic cycle
  exercises the runtime cycle check, so AC-16 was added.

## Conformance and Remaining Work

- **Requested:** the failing upstream runs once per evaluation, at any depth and capacity. Done.
- **Approved:** AC-1 to AC-16. All are covered by passing tests: `tests/evaluation_session.rs`,
  `tests/deep_chain_evaluation.rs`, `tests/dependency_audit_integration.rs`, and unit tests in
  `evaluation_session.rs`, `assets.rs` and `plan.rs`.
- **Remaining:** only the two filed issues above.

## Validation

Run on 2026-10-12, at the commits listed in Phase 4 §Progress:

- `cargo test --no-fail-fast -p liquers-core --lib --tests`: **1604 passed, 0 failed**, including
  `evaluation_session` (11), `deep_chain_evaluation` (8, every deep test also run alone),
  `dependency_audit_integration` (42), `external_asset_manager`, `asset_cancellation`,
  `dependency_scheduling` and `manager_parametric`. After that run, only doc comments, a test
  comment and the removal of an unused helper changed; the `evaluation_session` unit tests and
  the core build (no warnings) were re-run after them.
- `cargo test --no-fail-fast -p liquers-lib --lib --tests`: **592 passed, 0 failed**.
- `cargo check -p liquers-core --all-features --lib --tests` and `cargo check -p liquers-axum`: clean.
- `cargo check -p liquers-py`: clean with `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` (the container
  has CPython 3.13; see CLAUDE.md).
- `cargo check -p liquers-core --target wasm32-unknown-unknown`: clean apart from five warnings
  that are identical at the pre-implementation commit `24a674d`.
- After `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
  (Node): **161 passed, 0 failed** in 18 suites.
- `python3 scripts/docs_index.py --check`: 0 errors. `validate_phase.py` passes for Phases 1-4.
- Not run: the browser-only `liquers-web` loops (`--features browser-tests`, Playwright e2e) and
  `scripts/check-build-matrix.sh`. No `cfg(feature)` or optional dependency changed, and the
  inline manager's wasm behaviour is covered by the Node loop.
