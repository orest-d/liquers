---
id: LIQUERS-PY
kind: design
title: Python integration of Liquers (liquers-py)
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
merged: 2026-10-17
area: [py]
issues: [PY-MODULES-NOT-DECLARED-IN-LIB, PY-PYO3-0-21-CANNOT-TARGET-PYTHON-3-13, PY-VALUE-ENUM-OVERSIZED, PY-VALUE-SERIALIZER-IS-A-STUB, PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND, FOREIGN-VALUE-CANNOT-BE-DESERIALIZED]
affects_docs: [specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/VALUE_TYPE_SYSTEM.md, specs/reference/COMMAND_DECLARATION.md, specs/guides/COMMAND_DESIGN_GUIDE.md, specs/reference/api/API_DOCS_GAP_ANALYSIS.md, specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md]
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
