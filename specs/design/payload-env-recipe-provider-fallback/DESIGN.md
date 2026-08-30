---
id: PAYLOAD-ENV-RECIPE-PROVIDER-FALLBACK
kind: design
title: Recipe-provider fallback for the payload environment
status: complete
area: [core/context]
gh_pr: []
issues: [CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC]
affects_docs: [specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md]
created: 2026-08-29
superseded_by:
---
# Recipe-provider fallback for the payload environment

Design tracking for [`issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md`](../../issues/CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC.md), prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No `workflow:`
marker: this is a simplified transitional design whose required phases are the two written here
plus whatever the approval gate authorizes. It is **not** opted into the `liquers-project`
artifact and approval contract.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
- [x] Approval gate (§5 of the autonomous procedure)
- [x] Phase 3: Examples, reproduction and tests
- [x] Phase 4: Implementation plan and execution
- [x] Phase 5: Documentation — [`phase5-documentation.md`](./phase5-documentation.md)

## Why this folder exists

`SimpleEnvironmentWithPayload::get_recipe_provider` panics where its three siblings fall back. Phase 1 corrects a claim in the issue about the struct's doc comment; Phase 2 chooses the sibling fallback and weighs fixing it now against leaving it to `environment-builder`.

## Relationship to `environment-builder`

The issue was filed during that design's preflight and is listed in its `issues:` set, but this is
separate work with its own scope and its own gate. Nothing here changes
[`design/environment-builder/`](../environment-builder/)'s phase documents, front-matter or
workflow marker.
