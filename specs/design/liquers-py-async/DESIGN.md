---
id: LIQUERS-PY-ASYNC
kind: design
title: Async evaluation and async commands in liquers-py
workflow: liquers-project
gh_pr: [181]
phase: implementation
readiness: ready
autofix: not-eligible
merged: 2026-10-26
area: [py]
issues: [PY-ASYNC-EVALUATION-NOT-SUPPORTED, PY-FREE-THREADED-SUPPORT-UNVERIFIED]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-10-18
---
# liquers-py-async — asyncio for the Python integration

Adds `ASYNCQ` and `ASYNCCMD` to [`liquers-py`](../liquers-py/DESIGN.md), split out of that design
on 2026-10-18 to keep it within size. It builds on the runtime, environment and command adapter of
`liquers-py`, which is complete. Phase 1 was approved on 2026-10-26. Pre-approved after Phase 2 on 2026-10-27 (`proceed all`): Phases 3-4, the implementation and Phase 5.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
