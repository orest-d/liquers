---
id: LIQUERS-PY-PACKAGE
kind: design
title: Release wheels for liquers-py, built and tested in GitHub Actions
workflow: liquers-project
status: draft
phase: examples
readiness: ready
autofix: not-eligible
area: [py, build]
issues: [PY-NO-PUBLISHED-WHEELS]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/PROJECT_OVERVIEW.md, specs/reference/JAVASCRIPT_INTEGRATION.md, liquers-py/README.md, specs/README.md]
created: 2026-11-01
---
# liquers-py-package — release wheels for liquers-py

The release half of the `PACKAGE` feature for liquers-py (the build half was M0): milestone M5 of
the liquers-py plan, together with [`liquers-py-records`](../liquers-py-records/DESIGN.md) and
[`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md). Full form (`L`).

Phase 1 written and reviewed twice on 2026-11-01, revised after Orest's decisions and reviewed twice again, and approved the same day.

Phases 2 and 3 written on 2026-11-01, each reviewed twice (conformity, soundness) and revised.

**Pre-approved after Phase 1 on 2026-11-01** (Orest: "proceed all"): Phases 2-4, the implementation and Phase 5 run without gates; reviews still run, user decisions go to the Phase 1 decision log, and work stops before implementation only if one of them needs Orest.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
