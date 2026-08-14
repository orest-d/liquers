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
`~B<bin>~` for numeric entities, `~n<name>~` for named ones (`n` for "named"). Alternatives considered are in Annex A
of the Phase 1 document; the curated entity set is Annex B.

Key Phase 1 findings, for anyone picking this up cold:

- **The long form must be entered only on an opener letter that legacy text cannot produce.**
  Measured at HEAD: `f-~Hexampledotcom~~` parses and means `https://exampledotcom~`, so a bare
  `~<name>~` named entity would silently change an existing query's meaning. `~U ~D ~O ~B ~n` are
  all rejected today, which is what makes them safe.
- **`entities.rs` is reserved for named entities**; the general escaping algorithm and the numeric
  codec go in a new `escape.rs`. See the Crate Placement note for why this still satisfies the
  issue's one-definition requirement.
- **AST representation of entities is out of scope** — filed as `QUERY-AST-DISCARDS-ENTITIES`.
- **The parser widens to `char::is_alphanumeric()`, the encoder emits pure ASCII.** Widening is the
  non-breaking direction (`f-Ł` parses today); ASCII output keeps queries safe through ASCII-only
  systems. Consequence: liquers does not normalize, so composed and decomposed `café` stay two
  different values with two different canonical spellings.
- **A character with a curated entity is always encoded as that entity**, even when `~U<hex>~` is
  shorter. This makes the curated set a frozen compatibility surface — adding a name later changes
  canonical text and invalidates derived keys — so the tier boundaries in Annex B are a decision to
  take now, not a knob to turn later.
- **The full/curated split is `cfg(not(target_arch = "wasm32"))`, not a cargo feature.** A feature
  cannot express it: `liquers-lib` and `liquers-store` both pull `liquers-core` with defaults on, so
  unification puts the table back in the wasm bundle whatever `liquers-web` declares. Everything any
  build encodes, every build decodes, because the encoder only emits curated names and those are
  compiled on every target.
- **`encode_token` stays infallible.** `&str` guarantees every `char` is a scalar value and every
  scalar value has a `~U<hex>~` spelling, so no input is unrepresentable. Errors belong to the
  decoder (out-of-range, surrogate, unknown name, missing terminator).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
