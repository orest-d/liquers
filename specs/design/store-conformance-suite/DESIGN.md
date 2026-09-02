---
id: STORE-CONFORMANCE-SUITE
kind: design
title: An implemented conformance suite, a completed contract, and a store implementation guide
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/backends, web, docs]
gh_pr: []
issues: [STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE, CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS, CORE-SYNC-STORE-TRAIT-OBSOLETE, DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS]
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE, LANGUAGE-INTEGRATION_GUIDE, STORE_FACTORY_GUIDE, STORE_CONFIG_FSD]
created: 2026-09-02
superseded_by:
---
# An implemented conformance suite, a completed contract, and a store implementation guide

**Created:** 2026-09-02

## Phase Status

- [x] Phase 1: High-Level Design — awaiting approval
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Fixes `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` (P1, L). Three deliverables:

1. Complete `specs/reference/STORE_SEMANTICS.md` — the contract. Three rows are still ⚠.
2. Implement `liquers_core::store_conformance` — the suite, runtime-agnostic so it runs natively
   and under `wasm32`, and applied to all seven in-tree implementations.
3. Write `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the operational counterpart, modelled on
   `LANGUAGE-INTEGRATION_GUIDE.md` but with the suite *implemented* rather than fixed as
   appendix pseudocode.
4. Ship a validation tool that builds a store router from a YAML document, runs the suite and
   prints the report.

Contract, guide and suite stay synchronized through shared rule IDs, asserted by a test.

Settled at the Phase 1 gate: `keys()` returns data keys plus directories plus the prefix and every
returned key starts with the prefix; `removedir` is a postcondition (`Ok` means the directory is
gone), from which recursion follows; guide, contract and code share one vocabulary; the
synchronous `Store` trait is out of scope and obsolete (issue filed), though the contract stays
trait-neutral against its possible return for synchronous realms; a store failing its own suite is
fixed here unless the fix is `M` or larger. The suite reports rather than panics, and a store's
test asserts against the report with a declared list of rules it may fail; the report is also
obtainable directly for debugging. A validation tool in `liquers-store` builds a router from a YAML
document and prints the report. The suite never constructs stores — the caller supplies a fixture,
the guide carries the per-type recipes, and `StoreFactory` gains an additive, defaulted fixture
constructor so a type named in a document can be tested without one. Every rule declares whether it
is read-only or potentially destructive; the tool runs the read-only half by default, the report
distinguishes "not run" from "passed", and the guide states the precautions (temporary folder or
throwaway database, expendable store, no third-party service without explicit permission).

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
