---
id: LIQUERS-PY-UI-WINDOW
kind: design
title: "Liquers UI window from liquers-py: open the egui desktop UI over a Python environment"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, lib/egui, build]
issues: [PY-CANNOT-OPEN-A-LIQUERS-UI-WINDOW]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/UI_INTERFACE_FSD.md, docs/guides/python/11-ui.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-06
---
# liquers-py-ui-window — open the Liquers desktop UI from Python

The window half of the `UIUSE` feature for liquers-py: milestone M7 of the liquers-py plan, over the
model of [`liquers-py-ui`](../liquers-py-ui/DESIGN.md), which lands first, with
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md).
Full form (`L`). Requires a reusable native egui app in `liquers-lib` (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`,
Phase 1 question 2), its own design and PR first.
Based on `main` at `bd36cf1`; M6 (PRs #257, #259–#263) is still open and lands first.

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
