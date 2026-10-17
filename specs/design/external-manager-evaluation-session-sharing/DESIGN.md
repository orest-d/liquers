---
id: EXTERNAL-MANAGER-EVALUATION-SESSION-SHARING
kind: design
title: An external asset manager can schedule a dependency into its parent's evaluation session
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION]
created: 2026-10-17
---
# An external asset manager can schedule a dependency into its parent's evaluation session

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4
are written but not approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

Within one evaluation, `Context` pins each dependency it resolves in the evaluation session
(`liquers-core/src/evaluation_session.rs`). For a dependency's own upstream to be pinned in the
same session, the manager gives the dependency its parent's session before it starts. The built-in
managers do this with `AssetRef::adopt_evaluation_session` (`liquers-core/src/assets.rs`), which is
`pub(crate)` and takes the `pub(crate)` type `EvaluationSessionRef`. An external manager
(`liquers-core/tests/common/minimal_manager.rs` `MinimalInlineAssetManager`) cannot, so a diamond's
shared upstream may run once per branch and a volatile upstream can have two values in one
evaluation.

### Problem Example

```
v.txt  = tick            (volatile)
a.txt  = -R/data/v.txt/-/upper
b.txt  = -R/data/v.txt/-/upper
c.txt  = -R/data/a.txt/-/join-~X~-R/data/b.txt~E
```

Evaluating `-R/data/c.txt` on `MinimalInlineAssetManager`: today `a` and `b` each begin a session
of their own, so `tick` may run twice and `c` may join two different values. Should be: `tick` runs
once and both halves of `c` are equal, as on the built-in managers
(`liquers-core/tests/evaluation_session.rs` `volatile_scenario`).

### Scope and Acceptance Criteria

- **AC-1** Shared session on an external manager
  - WHEN an external manager's `get_dependency_asset` schedules the dependency into the parent's
    session through the public API, and evaluates the Problem Example
  - THEN `tick` runs once and `c`'s two halves are equal
- **AC-2** Adoption rules are unchanged
  - WHEN the dependency already belongs to a live session, or has started
  - THEN the call does nothing (rule 5 of the evaluation session: joined, not restarted)
- **AC-3** No session type leaks
  - WHEN an external crate uses the new method
  - THEN it names no session type; `EvaluationSession` stays crate-private

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` method to `AssetRef` in `liquers-core`
  (rule 4).
- **Leading issue:** **Proposed resolution - API shape:** which public surface lets an external
  manager share the session.
- **Explanation:** All candidate shapes work; they differ in what becomes public and in whether
  existing external managers break. The recommended one adds one method and breaks nothing.
- **Open questions:**
  1. **Proposed resolution - API shape.** Recommended: a new
     `pub async fn AssetRef::join_evaluation_session_of(&self, parent: &AssetRef<E>)` that copies
     `parent`'s live session under the existing adoption rules. Alternatives: (b) make
     `adopt_evaluation_session` and `EvaluationSessionRef` public (exposes the session type and its
     lifetime rules); (c) pass the session to `AssetManager::get_dependency_asset` (breaks every
     external manager's signature); (d) let `Context` adopt the session after the manager returns
     (too late: the trait's default `get_dependency_asset` calls `get_asset`, which may already have
     started the asset).

### Design Dependencies

- `requires` none open. `dependency-resolved-once-per-evaluation` and
  `external-manager-replacement-surface` (both `complete`) define the contract and the external
  surface; this extends the latter without reopening it (overlap rule E4).

### Consolidated Findings

The public method wraps the existing crate-private adoption; the built-in managers can keep calling
`adopt_evaluation_session` directly. `MinimalInlineAssetManager` must override
`get_dependency_asset` (it inherits the `get_asset` default today) to call the method before
`run_inline`.

## Phase 2: Architecture

### Solution

Under the recommended answer:

```rust
impl<E: Environment> AssetRef<E> {
    /// Schedules this asset into `parent`'s evaluation session, if `parent` has a live one and
    /// this asset has neither a live session nor a started run. Call it before the asset can start.
    pub async fn join_evaluation_session_of(&self, parent: &AssetRef<E>);
}
```

Body: `if let Some(s) = parent.evaluation_session().await { self.adopt_evaluation_session(&s).await }`.
Lock order: `parent`'s read lock is released before `self`'s write lock is taken, as in the
built-in managers. The trait doc of `get_dependency_asset` replaces its "an external manager
cannot" paragraph with a pointer to the method.

In `liquers-core/tests/common/minimal_manager.rs`, override `get_dependency_asset`: resolve the
asset as `get_asset` does, but for a pre-run asset call `join_evaluation_session_of(parent)`
before `run_inline`. The same in `get_dependency_asset_with_payload`.

### Changes

`liquers-core/src/assets.rs` (the method, the trait doc), `minimal_manager.rs`,
`specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` and `specs/reference/DEPENDENCIES_STATUS.md`
(where they say only crate code can adopt), History rows. Check `liquers-py` and `liquers-web`
for `AssetRef` wrappers (none expected to need the method).

### Risks

A manager that calls the method after starting the asset gets no effect (AC-2), which the doc
states. Certainty: high for the mechanism; the shape is the open decision.

## Phase 3: Examples and Tests

### Examples

The Problem Example; secondary: a failing shared upstream in a lattice runs once
(`common::chain_fixture::lattice`).

### Tests

In `liquers-core/tests/external_asset_manager.rs`, recipes from `common::chain_fixture`
(`recipe_store`) under `env_over`:

- `external_manager_volatile_dependency_has_one_value_per_evaluation` — AC-1
- `external_manager_shared_failing_root_runs_once_in_lattice` — AC-1
- `assets::tests::join_evaluation_session_of_keeps_an_existing_session` — an asset already in a
  live session, or `Processing`, keeps its session — AC-2, AC-3 (the test calls only the public
  method)

Run: `cargo test -p liquers-core --test external_asset_manager` and
`cargo test -p liquers-core --lib assets::tests::join_evaluation_session_of`.

## Phase 4: Implementation Plan

### Steps

- [ ] 0. Decision on the API shape recorded in Phase 1 — no code before it
- [ ] 1. Failing tests — commands above (AC-1 tests fail on the external manager)
- [ ] 2. `liquers-core/src/assets.rs` `AssetRef::join_evaluation_session_of` and the
  `get_dependency_asset` doc — unit test passes
- [ ] 3. `minimal_manager.rs` `get_dependency_asset` / `get_dependency_asset_with_payload`
  overrides — integration tests pass. Rollback: revert steps 2-3.
- [ ] 4. Guide and reference updates with History rows; close the issue —
  `python3 scripts/docs_index.py --check`

### Validation

`cargo test -p liquers-core --lib --tests`; `bash scripts/check-build-matrix.sh` is not needed (no
feature gates). Final diff review.
