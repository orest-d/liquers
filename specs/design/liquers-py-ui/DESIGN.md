---
id: LIQUERS-PY-UI
kind: design
title: "Liquers UI from liquers-py: open, drive and observe the egui desktop UI"
workflow: liquers-project
status: draft
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, lib/ui, lib/egui]
issues: [PY-CANNOT-USE-A-LIQUERS-UI]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-06
---
# liquers-py-ui — open, drive and observe the Liquers desktop UI

The `UIUSE` feature for liquers-py: milestone M7 of the liquers-py plan, together with
[`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md) and [`liquers-py-modules`](../liquers-py-modules/DESIGN.md). Full form (`XL`). Requires a reusable native egui app in `liquers-lib` (`EGUI-HAS-NO-REUSABLE-NATIVE-APP`, Phase 1 question 2), its own design and PR first.
Based on `main` at `bd36cf1`; M6 (PRs #257, #259–#263) is still open and lands first.

Phase 1 written on 2026-11-06.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
