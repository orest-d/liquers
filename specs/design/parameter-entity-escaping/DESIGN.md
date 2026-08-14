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

Resolves `PARAMETER-ESCAPING-INCOMPLETE` (P0). Syntax **decided**: `~U<hex>~` `~D<dec>~` `~O<oct>~`
`~B<bin>~` for numeric entities, `~x<name>~` for named ones. Alternatives considered are in Annex A
of the Phase 1 document; the curated entity set is Annex B.

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
  parses today, so narrowing is the breaking direction. Still open.
- **The named-table cargo feature gates decoding only.** The encoder's repertoire is frozen and
  feature-independent, because query text is identity (asset and cache keys): if a
  `default-features = false` build encoded from a smaller table, two builds would produce different
  canonical text for the same value. The feature is additive — curated set always compiled,
  `entities-html5` extends it — so `default-features = false` is the restriction mechanism.
- **`encode_token` stays infallible.** `&str` guarantees every `char` is a scalar value and every
  scalar value has a `~U<hex>~` spelling, so no input is unrepresentable. Errors belong to the
  decoder (out-of-range, surrogate, unknown name, missing terminator).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
