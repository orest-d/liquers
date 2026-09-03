---
id: SIDECAR-COLLIDING-KEYS
kind: design
title: Sidecar-colliding keys refused by the path builders
workflow: liquers-project
status: in_review
phase: implementation
area: [core/store, store/backends, docs]
gh_pr: []
issues: [CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS, STORE-METADATA-LAYOUT-HARDCODED-PER-STORE, CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS]
affects_docs: [STORE_SEMANTICS, STORE_IMPLEMENTATION_GUIDE]
created: 2026-09-03
superseded_by:
---
# Sidecar-colliding keys refused by the path builders

**Created:** 2026-09-03

## Phase Status

- [x] Phase 1: High-Level Design — approved 2026-09-03
- [x] Phase 2: Solution & Architecture — approved 2026-09-03
- [x] Phase 3: Examples & Testing — approved 2026-09-03
- [ ] Phase 4: Implementation Plan — awaiting approval
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

Fixes `CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS` (P1, M), found by conformance rule
`sidecar03` against fixture `C2` and recorded there as an allowed failure.

`AsyncFileStore` refuses a sidecar-colliding key in `is_supported` — a routing hint — and accepts it
everywhere else, so `set("collide.__metadata__")` overwrites the metadata of `collide`. The fix
moves the refusal into the path builders, as `STORE_SEMANTICS.md` §8 already promises and
`AsyncOpenDALStore` already implements.

Settled at the Phase 1 gate: the reserved-name rule covers **every segment**, not just the
filename, and both the suffix form (`x.__metadata__`) and the bare folder form (`__metadata__`) —
the latter because earlier Liquers versions kept metadata in a `__metadata__` folder and that
layout may need to return. Each store reserves what its own layout uses, in one predicate consulted
by `is_supported`, by every path builder, and by the listing filters. `.__lock__` is in scope for
the file stores; the refusal is `KeyNotSupported`; the obsolete synchronous `FileStore` is fixed
alongside; `PathMap` widens with them so the contract and both sidecar implementations agree. The
change is therefore cross-crate — `L`, not the issue's recorded `M`.

Making the metadata layout itself pluggable is **out of scope**, filed as
`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (P2, L), which records that its implementation must
revisit `is_supported` and the path builders: the reserved-name set becomes a property of the
configured layout rather than a constant.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
