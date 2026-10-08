---
id: ACTIVE-11
kind: design
title: Derived OpenDAL store arguments and offline S3 construction tests
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
area: [store/backends, store/config]
issues: [STORE-OPENDAL-ARGUMENTS-NOT-DERIVED]
affects_docs: [reference/STORE_CONFIG_FSD.md, guides/STORE_FACTORY_GUIDE.md]
created: 2026-09-03
---

# Design Tracking

> **Acceptance scenarios not defined.** This design predates acceptance scenarios
> (`specs/DOCS_STRUCTURE_GUIDE.md` §5.2.1). Consider updating it: state the Phase 1 acceptance
> criteria as `AC-<n>` WHEN/THEN scenarios and cite each from the Phase 3 test that proves it. Remove
> this note when you do; until then `docs_index.py --check` counts this design in its warning.

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation (executed 2026-10-07; awaiting approval)

Phases 1-4 were rewritten on 2026-10-05: the first version was generic template text, which the
post-Phase-4 review found too thin to implement from.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Still valid; no change.** `Cargo.lock` still locks
OpenDAL 0.55.0, `OpendalStoreFactory::common_arguments` (`liquers-store/src/store_factory.rs`
≈103) and `StoreArgumentInfo::derived` (`liquers-core/src/store_factory.rs` ≈114) are as the
design describes, and the `s3_01`/`s3_02` tests still do not exist. Readiness stays `ready`.
