# Phase 1: High-level design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The completed keyed-delegation hand-off exposes a precise internal distinction: delegation returns the owner state, and only that route must skip persistence.
- **Open questions:** None.

## Problem and outcome

After `AssetRef::evaluate_recipe` delegates to the owner, `evaluate_and_store` cannot distinguish that state from one computed locally and persists the owner's bytes a second time. Carry an internal evaluation outcome that records delegation and skip only that persistence attempt.

Acceptance criteria: default and immediate keyed delegation produce the owner value, execute the producer once, and call the backing store's value write once; ordinary recipe evaluation, explicit `set_state`, and persistence-status tracking remain unchanged.

## Scope and constraints

The change is confined to `liquers-core/src/assets.rs` and manager-parametric integration tests. It is an optimization, not a change to ownership, dependency records, stored metadata, or error propagation. Do not use `bound_owner_key()` as a blanket persistence gate because other non-owning paths may legitimately persist.

## Design Dependencies

- `keyed-delegation-hand-off` - **requires**: its completed hand-off establishes the delegation branch and the regression arrangements used here.
- `keyed-recipe-ownership` - **overlaps**: reuse its immutable key/owner semantics; do not infer ownership from mutable resolved recipes.

## Documentation assessment

Review `specs/reference/DEPENDENCIES_STATUS.md`; likely no change because persistence is not part of its hand-off contract. Update issue/design records and generated indexes.

## Consolidated Findings

The narrow result wrapper is safer than ownership gating: it preserves all existing persistence callers and makes the no-write behaviour causally tied to the delegation branch. The regression test needs a counting async store rather than command counts alone, because both the redundant and fixed paths run the producer once.
