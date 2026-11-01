---
id: LIQUERS-PY-RECORDS
kind: design
title: "Record views and sources in liquers-py: wrappers, chunked iteration and an Arrow exit"
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
autofix: not-eligible
area: [py, records]
issues: [PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON, PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON]
merged: 2026-11-01
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/RECORD_STREAMS.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/reference/PROJECT_OVERVIEW.md, liquers-py/README.md, specs/README.md]
created: 2026-11-01
---
# liquers-py-records — record views and sources from Python

The `RECORDS` sub-feature of `VALUE` for liquers-py: milestone M5 of the liquers-py plan, together
with [`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md) and
[`liquers-py-package`](../liquers-py-package/DESIGN.md). Full form (`L`). On 2026-11-01 Orest
decided Phase 1 question 1 for wrappers, so it took over `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`
from the bulk design `py-record-values-to-python`, now superseded.

Phase 1 written and reviewed twice on 2026-11-01, revised after Orest's decisions and reviewed twice again, and approved the same day.

Phases 2, 3 and 4 written on 2026-11-01, each reviewed twice (conformity, soundness; the Phase 4
conformity review included the consistency pass over all phases) and revised. The pre-implementation
check found no decision that needs Orest, so the design is `approved` under the pre-approval and
implementation starts.

**Pre-approved after Phase 1 on 2026-11-01** (Orest: "proceed all", given at the Phase 1 gate, earlier than the skill's Phase 2 offer, and covering Phases 2-5): Phases 2-4, the implementation and Phase 5 run without gates; reviews still run, user decisions go to the Phase 1 decision log, and work stops before implementation only if one of them needs Orest.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
