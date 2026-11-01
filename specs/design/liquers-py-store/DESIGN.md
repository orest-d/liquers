---
id: LIQUERS-PY-STORE
kind: design
title: Stores written in Python, and configured store routers in liquers-py
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
area: [py, core/store, store/config]
issues: [PY-NO-LANGUAGE-DEFINED-STORE]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/STORE_CONFIG_FSD.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-10-31
---
# liquers-py-store — stores written in Python, and configured routers

The `STORE` feature of liquers-py, directions 1, 2 and 3: milestone M4 of the liquers-py plan,
together with [`liquers-py-recipes`](../liquers-py-recipes/DESIGN.md). It is the design the plan's
Pre-M4 item 1 asked for, and it takes the Pre-M4 part B decisions (Phase 1, Design Readiness).
Full form, since the source issue is `L`.

Phase 1 approved by Orest on 2026-11-01 (`proceed all`). Pre-approved after Phase 1 on 2026-11-01: Phases 2-4, the implementation and Phase 5, with a stop before implementation only if the decision log needs a decision. (The skill offers `proceed all` at the Phase 2 gate; Orest gave it at Phase 1, so the remaining gates were run with their two reviewer passes and no stop.)

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
