---
id: LIQUERS-PY-ASYNC
kind: design
title: Async evaluation and async commands in liquers-py
workflow: liquers-project
status: draft
phase: high-level
area: [py]
issues: [PY-ASYNC-EVALUATION-NOT-SUPPORTED]
affects_docs: []
created: 2026-10-18
---
# liquers-py-async — asyncio for the Python integration

Adds `ASYNCQ` and `ASYNCCMD` to [`liquers-py`](../liquers-py/DESIGN.md), split out of that design
on 2026-10-18 to keep it within size. Starts after `liquers-py` Phase 4 is implemented, since it
builds on its runtime, environment and command adapter.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
