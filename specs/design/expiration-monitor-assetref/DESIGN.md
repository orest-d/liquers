---
id: EXPIRATION-MONITOR-ASSETREF
kind: design
title: Weak references in the expiration monitor
status: complete
area: [core/assets]
gh_pr: [9, 11]
issues: []
created: 2026-03-02
superseded_by:
---
# expiration-monitor-assetref Design Tracking

**Created:** 2026-02-28


## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

PR #9 was closed unmerged. The design landed anyway, through PR #11 (`expiration-safety`,
WP-3): `TimedAsset` holds a `WeakAssetRef` rather than a strong `AssetRef`
(`liquers-core/src/assets.rs:3347`), and `test_untrack_releases_strong_ref` guards it
(`liquers-core/src/assets.rs:6863`). That is why this design is `complete` with a PR that
never merged — §5.5's "needs a human" row, answered.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
