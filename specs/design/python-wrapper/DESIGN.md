---
id: PYTHON-WRAPPER
kind: design
title: Python wrapper architecture for liquers-py
status: complete
area: [py]
issues: []
created: 2026-03-02
superseded_by:
---

# Python Wrapper Design Tracking

**Status:** Complete — implemented as `liquers-py`.

This design predates the `liquers-designer` folder convention. It arrived as two top-level
documents, `PYTHON-WRAPPER-HIGH-LEVEL-DESIGN.md` and `PYTHON-WRAPPER-ARCHITECTURE.md`, which map
onto phases 1 and 2 and were renamed accordingly during the 2026-08-08 documentation migration.
There are no phase 3 or 4 documents; the implementation landed without them.

`complete` is assigned per `DOCS_STRUCTURE_GUIDE.md` §5.3 rule 1 — every phase *required when this
design was approved* is done, and at that time there was no phase set at all.

## Phase Status

- [x] Phase 1: High-Level Design — [`phase1-high-level-design.md`](./phase1-high-level-design.md)
- [x] Phase 2: Solution & Architecture — [`phase2-architecture.md`](./phase2-architecture.md)
- [ ] Phase 3: Examples & Testing — not written
- [ ] Phase 4: Implementation Plan — not written
- [x] Implementation Complete — `liquers-py/src/` carries the wrapper modules

## Notes

`PYTHON-BASIC-OBJECTS` — the feature brief covering the `query`, `metadata`, `plan`, `expiration`,
`dependencies` and `recipes` wrappers — was verified implemented during the migration triage and
is **not** in the issue set. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
