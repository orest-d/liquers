---
id: STORE-CONFORMANCE-SUITE
kind: design
title: An implemented conformance suite, a completed contract, and a store implementation guide
workflow: liquers-project
status: draft
phase: implementation
area: [core/store, store/backends, web, docs]
gh_pr: []
issues: [STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE, CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS, CORE-SYNC-STORE-TRAIT-OBSOLETE, DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS, STORE-CONFORMANCE-VALIDATION-TOOL, CORE-LISTDIR-KEYS-DEEP-TESTS-THE-WRONG-KEY]
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE, CONFORMANCE_TERMS, LANGUAGE-INTEGRATION_GUIDE, STORE_FACTORY_GUIDE, STORE_CONFIG_FSD]
created: 2026-09-02
superseded_by:
---
# An implemented conformance suite, a completed contract, and a store implementation guide

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design — approved 2026-09-02
- [x] Phase 2: Solution & Architecture — approved 2026-09-02
- [x] Phase 3: Examples & Testing — approved 2026-09-02
- [x] Phase 4: Implementation Plan — approved 2026-09-02
- [x] Implementation — steps 1, 2, 4-12, 14, 16 complete; step 15 deliberately not carried out — awaiting approval
- [ ] Phase 5: Documentation

## Notes

Fixes `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (P1, L). Three deliverables:

1. Complete `specs/reference/STORE_SEMANTICS.md` — the contract. Three rows are still ⚠.
2. Implement `liquers_core::store_conformance` — the suite, runtime-agnostic so it runs natively
   and under `wasm32`, and applied to all seven in-tree implementations.
3. Write `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the operational counterpart, modelled on
   `LANGUAGE-INTEGRATION_GUIDE.md` but with the suite *implemented* rather than fixed as
   appendix pseudocode.
A fourth deliverable — a validation tool building a store router from a YAML document — was
designed here and **deferred at the Phase 4 gate** to `STORE-CONFORMANCE-VALIDATION-TOOL` (P2, M),
which carries its decided design in full. That is what returns this project to a defensible `L`.

Contract, guide and suite stay synchronized through shared rule IDs, asserted by a test.

Settled at the Phase 1 gate: `keys()` returns data keys plus directories plus the prefix and every
returned key starts with the prefix; `removedir` is a postcondition (`Ok` means the directory is
gone), from which recursion follows; guide, contract and code share one vocabulary; the
synchronous `Store` trait is out of scope and obsolete (issue filed), though the contract stays
trait-neutral against its possible return for synchronous realms; a store failing its own suite is
fixed here unless the fix is `M` or larger. The suite reports rather than panics, and a store's
test asserts against the report with a declared list of rules it may fail; the report is also
obtainable directly for debugging. The suite never constructs stores — the caller supplies a fixture,
the guide carries the per-type recipes, and `StoreFactory` gains an additive, defaulted fixture
constructor so a type named in a document can be tested without one. Safety is three ordered levels — `read-only`,
`create-only` and `scratch` (only what this run created) — with each rule declaring the lowest it
can run at; a fourth, `unrestricted`, was specified in Phase 1 and removed at the Phase 3 gate when
the inventory showed no rule needs it; the report distinguishes "not run" from "passed" and names the level that
would run it. Level 3 is upheld by the rules on trust — check before write, no guard wrapper. Unit tests run at
fixture + scratch. Rules ask the fixture for key names rather than inventing them, which is what
lets the suite reach a specialized store (a view onto a database table, keyed by numeric row ID,
with no directories); such a store conforms to a subset, and many argued `NA`s are expected rather
than suspicious. The guide states the precautions (temporary folder or throwaway database,
expendable store, no third-party service without explicit permission).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
