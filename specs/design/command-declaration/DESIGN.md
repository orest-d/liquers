---
id: COMMAND-DECLARATION
kind: design
title: A language-neutral command declaration type
status: in_review
phase: architecture
area: [core/commands, web, py]
gh_pr: []
issues: [COMMAND-DECLARATION-FORMAT, STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE]
created: 2026-08-29
superseded_by:
---
# A language-neutral command declaration type

Design tracking for [`issues/COMMAND-DECLARATION-FORMAT.md`](../../issues/COMMAND-DECLARATION-FORMAT.md), prepared under
[`guides/autonomous_issue_fixing.md`](../../guides/autonomous_issue_fixing.md). No `workflow:`
marker: this is a simplified transitional design whose required phases are the two written here
plus whatever the approval gate authorizes. It is **not** opted into the `liquers-project`
artifact and approval contract.

## Phase status

- [x] Phase 1: High-level design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution and architecture — [`phase2-architecture.md`](./phase2-architecture.md)
      *(revised 2026-08-29: fix `CommandMetadata` rather than mirror it; `run` withdrawn pending
      the gate — see Phase 2 open question 1)*
- [ ] Approval gate (§5 of the autonomous procedure) — **awaiting a decision**
- [ ] Phase 3: Examples, reproduction and tests
- [ ] Phase 4: Implementation plan and execution
- [ ] Phase 5: Documentation

## Why this folder exists

`liquers-web` hand-parses a command declaration out of a `JsValue`, and a Python binding would
rewrite it. Phase 1 measures what stops `CommandMetadata` from serving as the declaration format —
five missing `#[serde(default)]` attributes, and three concepts it has no field for. Phase 2
specifies the serde fixes that close the first gap, a small `CommandBinding` type in `liquers-core`
for the second, and the `liquers-web` re-implementation over both.

A first draft of Phase 2 proposed a parallel `CommandDeclaration` struct mirroring `CommandMetadata`
field for field; review found it was largely a re-skin that also lost `presets`, `next`, `hints` and
`CommandDefinition::Alias`. It is kept in Phase 2 §Rejected alternatives rather than deleted.

## Relationship to `environment-builder`

The issue was filed during that design's preflight and is listed in its `issues:` set, but this is
separate work with its own scope and its own gate. Nothing here changes
[`design/environment-builder/`](../environment-builder/)'s phase documents, front-matter or
workflow marker.
