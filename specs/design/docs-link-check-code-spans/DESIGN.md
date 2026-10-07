---
id: DOCS-LINK-CHECK-CODE-SPANS
kind: design
title: The docs dead-link check ignores code spans and fenced blocks
status: complete
readiness: ready
area: [docs, build]
issues: [DOCS-LINK-CHECK-READS-CODE-SPANS]
created: 2026-10-06
---
# The docs dead-link check ignores code spans and fenced blocks

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

## Implementation (2026-10-06)

Implemented as planned (Wave 0 of `archive/2026-10-06-p2-p3-s-implementation-order-revised.md`): `scripts/docs_index.py` (`blank_code`, used by `relative_link_errors`), `BlankCodeTests` in `scripts/test_docs_index.py`, and the §7.2 check 9 sentence in `DOCS_STRUCTURE_GUIDE.md`. No other raw-text pass validates links: the `specs/README.md` issue-ID scan reads code spans on purpose.
