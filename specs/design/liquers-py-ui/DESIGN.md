---
id: LIQUERS-PY-UI
kind: design
title: "Liquers UI model in liquers-py: show, drive and observe UI elements"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, lib/ui]
issues: [PY-CANNOT-USE-A-LIQUERS-UI]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/UI_INTERFACE_FSD.md, specs/reference/PAYLOAD_GUIDE.md, docs/guides/python/11-ui.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-06
---
# liquers-py-ui — show, drive and observe a Liquers UI model

The model half of the `UIUSE` feature for liquers-py: milestone M7 of the liquers-py plan, together with
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) and
[`liquers-py-ui-window`](../liquers-py-ui-window/DESIGN.md), the window half, split from this design on 2026-11-06
(Phase 1 review) so the wheel decision does not hold up the model. Full form (`L`); needs no egui.
Based on `main` at `bd36cf1`; M6 (PRs #257, #259–#263) is still open and lands first.

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
