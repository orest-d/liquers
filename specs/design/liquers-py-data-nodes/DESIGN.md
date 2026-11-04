---
id: LIQUERS-PY-DATA-NODES
kind: design
title: "Data nodes in liquers-py: Python data read in place, and Liquers values read field by field"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, lib/value]
issues: [PY-STRUCTURES-ARE-OPAQUE-TO-DATA-NODE-READERS]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md, specs/guides/TEMPLATE_GUIDE.md]
created: 2026-11-04
---
# liquers-py-data-nodes — Python data read in place, and Liquers values read field by field

The `DATANODE` feature for liquers-py: milestone M6 of the liquers-py plan, together with
[`liquers-py-templates`](../liquers-py-templates/DESIGN.md) (which reads its `data` through this
adapter) and [`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md). Full form (`L`).
Requires the core hook of `foreign-value-data-node` and the closure primitive of
`AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE` (for `env.data_node`), each in its own PR first.

Phase 1 written and reviewed twice on 2026-11-04. At the gate Orest decided question 2 (opt-in);
revised and waiting at the Phase 1 gate.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
