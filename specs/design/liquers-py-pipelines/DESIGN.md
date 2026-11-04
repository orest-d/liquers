---
id: LIQUERS-PY-PIPELINES
kind: design
title: "Pipelines in liquers-py: evaluate, convert, edit and register pipelines from Python"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, core/commands]
issues: [PIPELINES-NOT-IN-LANGUAGE-BINDINGS, PY-HAS-NO-VALIDATING-QUERY-EDITOR]
merged: 2026-11-04
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md, specs/reference/PIPELINES.md, specs/guides/PIPELINE_GUIDE.md]
created: 2026-11-04
---
# liquers-py-pipelines — evaluate, convert, edit and register pipelines from Python

The `PIPELINE` feature for liquers-py: milestone M6 of the liquers-py plan, together with
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md) and
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md). Full form (`L`). Implements
`PIPELINES-NOT-IN-LANGUAGE-BINDINGS`, narrowed to Python (the JavaScript half is
`WEB-CANNOT-USE-PIPELINES`), and
`PY-HAS-NO-VALIDATING-QUERY-EDITOR`, split from `QUERY-BUILDER-NOT-IN-BINDINGS` and merged into this
design on 2026-11-04 (Phase 1 Decision 1).

Phase 1 written and reviewed twice on 2026-11-04. At the gate Orest agreed questions 1, 2, 5 and 6;
waiting at the Phase 1 gate for `proceed`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
