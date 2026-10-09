---
id: TYPE-INFO-WRITE-ONLY-FORMATS
kind: design
title: TypeInfo declares write-only data formats
status: abandoned
area: [core/value]
issues: [TYPE-INFO-CANNOT-DECLARE-WRITE-ONLY-FORMATS]
created: 2026-10-06
---
# TypeInfo declares write-only data formats

> **Abandoned on 2026-10-08: folded into `DATA-FORMAT-CONSTANTS-AND-TOOLING`** by maintainer decision
> (backlog compaction D8). Write-only formats will be designed as part of that feature's data-format
> vocabulary rather than as a separate `TypeInfo` field. The subset-list representation in Phase 2
> and the `RecordView` `html` example remain input for that design.

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

Produced under [`guides/autonomous_bulk_design.md`](../../guides/autonomous_bulk_design.md):
the first four phases, reviewed without phase approval. Not an approval and not an implementation.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution and Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
