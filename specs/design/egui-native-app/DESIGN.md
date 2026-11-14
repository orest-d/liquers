---
id: EGUI-NATIVE-APP
kind: design
title: "A reusable native UI driver and egui window in liquers-lib"
workflow: liquers-project
gh_pr: [268]
status: complete
readiness: ready
autofix: not-eligible
area: [lib/egui, lib/ui]
issues: [EGUI-HAS-NO-REUSABLE-NATIVE-APP]
affects_docs: [specs/reference/UI_INTERFACE_FSD.md, docs/guides/rust/11-ui.md, specs/README.md]
created: 2026-11-10
---
# A reusable native UI driver and egui window in liquers-lib

The prerequisite of [`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md) (its Phase 1
question 2, whose direction was approved with that design's Phase 1 on 2026-11-07) and of the
background driver of [`liquers-py-ui`](../liquers-py-ui/DESIGN.md). Full form since Phase 3 (Phase 1, Scope Changes). Phases 1 and 2
written together on 2026-11-10 and reviewed twice with the Python designs' Phase 2 (conformity and
records; codebase and soundness); findings applied. Phases 1 and 2 approved
2026-11-11 with the Python designs' Phase 2 (Orest: "proceed all", which pre-approves Phases 3
and 4, the implementation and Phase 5). Pre-approved after Phase 2 on 2026-11-11.
Implemented 2026-11-11 on PR #268 (the first commit group). Phase 5 written 2026-11-11 and approved 2026-11-14 (Orest: "complete and merge the design"): `status: complete`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
