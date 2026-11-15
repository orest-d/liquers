---
id: PY-PACKAGE-SUBMODULES
kind: design
title: Public submodules for the liquers Python package, mirroring the Rust modules
workflow: liquers-project
status: approved
phase: implementation
area: [py]
issues: [PY-API-IS-ONE-FLAT-MODULE]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, docs/guides/python/01-introduction.md, specs/guides/API_DOCUMENTATION_GUIDE.md]
created: 2026-11-14
---
# Public submodules for the liquers Python package

Full form (`L`), converted from the compact form at Phase 2 (Phase 1 *Scope Changes*). Source
issue `PY-API-IS-ONE-FLAT-MODULE`; draft PR #272.

Phase 1 written and reviewed by the author on 2026-11-14 (compact form); Orest's gate answers
applied the same day. Phase 1 approved 2026-11-14 (Orest: "proceed").

Phase 2 written 2026-11-14 and reviewed twice (conformity and records; codebase and soundness);
findings applied.

Phase 2 approved and pre-approved after Phase 2 on 2026-11-15 (Orest: "proceed all"): Phases 3
and 4, the implementation and Phase 5 run without gates; Phase 5 results are presented before
`status: complete`.

Phases 3 and 4 written 2026-11-15 and reviewed twice (conformity, records and the consistency
pass over all phases; codebase and soundness); findings applied. Problem filed:
`PY-ERROR-INSTANCES-CANNOT-BE-PICKLED` (pre-existing, out of scope).

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
