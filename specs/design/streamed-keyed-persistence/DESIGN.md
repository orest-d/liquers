---
id: STREAMED-KEYED-PERSISTENCE
kind: design
title: A keyed asset whose value streams is hashed and stored as it is encoded
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets, core/store]
issues: [STREAMED-VALUE-IS-COLLECTED-BEFORE-IT-IS-STORED]
affects_docs: [reference/ASSETS.md, reference/ASSET_LIFECYCLE.md, guides/SERIALIZATION_GUIDE.md]
created: 2026-10-26
---
# A keyed asset whose value streams is hashed and stored as it is encoded

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. When a keyed, computed asset's value encodes as a stream, the
encoding is written with `set_stream` and hashed on the way, and the asset becomes readable with
its content version when the write completes. Builds on three unfinished designs:
`value-serialization-streaming`, `store-streaming-read-write` and `shared-metadata-and-bytes`.

## Phase Status

- [x] Phase 1: High-Level Design (written, not approved)
- [x] Phase 2: Architecture (written, not approved)
- [x] Phase 3: Examples and Tests (written, not approved)
- [x] Phase 4: Implementation Plan (written, not approved)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
