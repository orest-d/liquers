---
id: PARAMETER-ENTITY-ESCAPING
kind: design
title: Parameter entity escaping (numeric and named tilde entities)
workflow: liquers-project
status: draft
phase: high-level
area: [core/query]
gh_pr: []
issues: [PARAMETER-ESCAPING-INCOMPLETE]
affects_docs: [specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md, specs/reference/PROJECT_OVERVIEW.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md]
created: 2026-08-14
superseded_by:
---
# parameter-entity-escaping Design Tracking

**Created:** 2026-08-14

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Resolves `PARAMETER-ESCAPING-INCOMPLETE` (P0). Phase 1 recommends `~U<hex>~`/`~D<dec>~` for numeric
entities and `~x<name>~` for named ones, with alternatives in Annex A of the Phase 1 document.

Key Phase 1 findings, for anyone picking this up cold:

- **The long form must be entered only on an opener letter that legacy text cannot produce.**
  Measured at HEAD: `f-~Hexampledotcom~~` parses and means `https://exampledotcom~`, so a bare
  `~<name>~` named entity would silently change an existing query's meaning. `~U ~D ~O ~B ~x` are
  all rejected today, which is what makes them safe.
- **`entities.rs` is reserved for named entities**; the general escaping algorithm and the numeric
  codec go in a new `escape.rs`. See the Crate Placement note for why this still satisfies the
  issue's one-definition requirement.
- **AST representation of entities is out of scope** — filed as `QUERY-AST-DISCARDS-ENTITIES`.
- The `c as u8` fix should **widen** to `char::is_alphanumeric()`, not narrow to ASCII: `f-Ł`
  parses today, so narrowing is the breaking direction.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
