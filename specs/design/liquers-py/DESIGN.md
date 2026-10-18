---
id: LIQUERS-PY
kind: design
title: Python integration of Liquers (liquers-py)
workflow: liquers-project
status: in_review
phase: architecture
area: [py]
issues: [PY-VALUE-ENUM-OVERSIZED, PY-VALUE-SERIALIZER-IS-A-STUB, PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND]
affects_docs: [specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md]
created: 2026-10-17
---
# liquers-py — Python integration

Modernizes `liquers-py` into a conformant integration for the Python baseline profile plus
`ASYNCQ` and `ASYNCCMD` (Phase 1). Supersedes [`python-wrapper`](../python-wrapper/DESIGN.md).
Plan of record: [`archive/2026-10-17-liquers-py-implementation-plan.md`](../../archive/2026-10-17-liquers-py-implementation-plan.md).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
