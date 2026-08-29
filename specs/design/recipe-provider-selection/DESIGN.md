---
id: RECIPE-PROVIDER-SELECTION
kind: design
title: Selecting a recipe provider by name
status: in_review
phase: architecture
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
- [ ] Approval gate (§5 of the autonomous procedure) — **awaiting a decision**
- [ ] Phase 3: Examples, reproduction and tests
- [ ] Phase 4: Implementation plan and execution
- [ ] Phase 5: Documentation

## Why this folder exists

The issue asks for a named lookup so a configuration document can say `recipes: default`. Phase 1 states what that means and what it is not; Phase 2 chooses a plain serde enum over a `StoreFactory`-style registry, and records why the store precedent does not transfer.

## Relationship to `environment-builder`

The issue was filed during that design's preflight and is listed in its `issues:` set, but this is
separate work with its own scope and its own gate. Nothing here changes
[`design/environment-builder/`](../environment-builder/)'s phase documents, front-matter or
workflow marker.
