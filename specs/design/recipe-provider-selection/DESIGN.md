---
id: RECIPE-PROVIDER-SELECTION
kind: design
title: Selecting a recipe provider by name
status: complete
phase:
area: [core/assets, web]
gh_pr: []
issues: [RECIPE-PROVIDER-BY-NAME]
created: 2026-08-29
superseded_by:
---
# Selecting a recipe provider by name

Design tracking for [`issues/RECIPE-PROVIDER-BY-NAME.md`](../../issues/RECIPE-PROVIDER-BY-NAME.md), prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No `workflow:`
marker: this is a simplified transitional design whose required phases are the two written here
plus whatever the approval gate authorizes. It is **not** opted into the `liquers-project`
artifact and approval contract.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
- [x] Approval gate (§5 of the autonomous procedure) — **approved 2026-08-29**, see below
- [x] Phase 3: Examples, reproduction and tests — [`phase3-tests.md`](./phase3-tests.md)
- [x] Phase 4: Implementation plan and execution — [`phase4-implementation.md`](./phase4-implementation.md)
- [x] Phase 5: Documentation — [`phase5-documentation.md`](./phase5-documentation.md)

## Gate decision

The maintainer approved the enum and settled Phase 2's open questions on 2026-08-29:

- **The set stays closed at two.** Only `default` and `trivial` are named. Custom recipe providers
  are too varied to standardize at present and remain specified ad hoc, by passing the provider
  value to the environment. This confirms Phase 2's rejection of a `RecipeProviderFactory`, now as
  a decision rather than a deferral.
- **`trivial` gains the input aliases `none` and `no_recipes`.** Serialization still emits
  `trivial`, so this widens what a document may say without widening the emitted format.
- **Q1 — `#[default]` on `Default`**, as Phase 2 recommended and the issue specifies.
- **Q2 — both `provider()` and `boxed_provider()` ship**, as Phase 2 recommended.

## Why this folder exists

The issue asks for a named lookup so a configuration document can say `recipes: default`. Phase 1 states what that means and what it is not; Phase 2 chooses a plain serde enum over a `StoreFactory`-style registry, and records why the store precedent does not transfer. Phases 3–5 test, implement and document `RecipeProviderChoice` in `liquers-core/src/recipes.rs`.

## Relationship to `environment-builder`

The issue was filed during that design's preflight and is listed in its `issues:` set, but this is
separate work with its own scope and its own gate. Nothing here changes
[`design/environment-builder/`](../environment-builder/)'s phase documents, front-matter or
workflow marker.
