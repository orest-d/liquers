---
id: LIQUERS-PY
kind: design
title: Python integration of Liquers (liquers-py)
workflow: liquers-project
status: approved
phase: examples
readiness: needs-decision
autofix: not-eligible
merged: 2026-10-17
area: [py]
issues: [PY-VALUE-ENUM-OVERSIZED, PY-VALUE-SERIALIZER-IS-A-STUB, PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND, FOREIGN-VALUE-CANNOT-BE-DESERIALIZED]
affects_docs: [specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/ASSETS.md]
created: 2026-10-17
---
# liquers-py — Python integration

Modernizes `liquers-py` into a conformant integration for the Python baseline profile (Phase 1).
`ASYNCQ` and `ASYNCCMD` moved to [`liquers-py-async`](../liquers-py-async/DESIGN.md) on 2026-10-18. Supersedes [`python-wrapper`](../python-wrapper/DESIGN.md).
Plan of record: [`archive/2026-10-17-liquers-py-implementation-plan.md`](../../archive/2026-10-17-liquers-py-implementation-plan.md).

Pre-approved after Phase 2 on 2026-10-18 (owner: run all phases without gates, with full
reviews). The decision log is Phase 1's `## Design Readiness`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
