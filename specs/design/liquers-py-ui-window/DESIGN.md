---
id: LIQUERS-PY-UI-WINDOW
kind: design
title: "Liquers UI window from liquers-py: open the egui desktop UI over a Python environment"
workflow: liquers-project
status: in_review
phase: documentation
readiness: needs-decision
autofix: not-eligible
area: [py, lib/egui, build]
issues: [PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/UI_INTERFACE_FSD.md, docs/guides/python/11-ui.md, docs/guides/python/02-installing.md, specs/guides/GITHUB_ACTIONS_GUIDE.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-06
---
# liquers-py-ui-window — open the Liquers desktop UI from Python

The window half of the `UIUSE` feature for liquers-py: milestone M7 of the liquers-py plan, over the
model of [`liquers-py-ui`](../liquers-py-ui/DESIGN.md), which lands first, with
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md).
Full form (`L`). Requires a reusable native egui app in `liquers-lib` (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`,
Phase 1 question 2): design [`egui-native-app`](../egui-native-app/DESIGN.md), the first commit group of the same PR
(Phase 1, Scope Changes).
Based on `main` at `0b01a20` (M6 and the M7 server merged).

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.
Phase 1 approved 2026-11-07 (Orest: "proceed", gate after each phase). Phase 2 written 2026-11-10,
after Orest asked to "design the UI - egui support". Phase 2 approved 2026-11-11 (Orest: "proceed
all", which pre-approves Phases 3 and 4, the implementation and Phase 5); its question 1 (egui in
every wheel) is unanswered and assumed "yes". Pre-approved after Phase 2 on 2026-11-11.
Phases 3 and 4 written 2026-11-11 from the working implementation.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
