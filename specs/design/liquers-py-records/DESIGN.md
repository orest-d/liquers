---
id: LIQUERS-PY-RECORDS
kind: design
title: "Record views and sources in liquers-py: wrappers, chunked iteration and an Arrow exit"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, records]
issues: [PY-RECORD-SOURCES-CANNOT-BE-TRAVERSED-FROM-PYTHON, PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON]
merged: 2026-11-01
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/reference/RECORD_STREAMS.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-01
---
# liquers-py-records — record views and sources from Python

The `RECORDS` sub-feature of `VALUE` for liquers-py: milestone M5 of the liquers-py plan, together
with [`liquers-py-stubs`](../liquers-py-stubs/DESIGN.md) and
[`liquers-py-package`](../liquers-py-package/DESIGN.md). Full form (`L`). On 2026-11-01 Orest
decided Phase 1 question 1 for wrappers, so it took over `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON`
from the bulk design `py-record-values-to-python`, now superseded.

Phase 1 written and reviewed twice on 2026-11-01. Orest decided its questions the same day (Phase 1 Decisions); revised and reviewed twice again, it is back at the Phase 1 gate, waiting for `proceed`.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
