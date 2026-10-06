---
id: MANIFEST-FOLDER-LISTING-INVALIDATION
kind: design
title: Invalidating ManifestRecipeProvider's folder cache when a directory changes
status: superseded
area: [records]
issues: [MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES]
created: 2026-10-04
superseded_by: recipe-provider-listing-contract
---
# manifest-folder-listing-invalidation Design Tracking

> **Superseded on 2026-10-05** by [`recipe-provider-listing-contract`](../recipe-provider-listing-contract/DESIGN.md), Part B (folder-cache invalidation). The maintainer
> merged this design with the two others it depended on after the post-Phase-4 review; the merged
> design carries this content with the review's corrections and owns this design's source issue.
> This folder is kept for the reasoning; do not implement from it.

Autonomous bulk design (`guides/autonomous_bulk_design.md`) for
`MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES`. The issue was previously linked to the
`record-streams` project (complete), which recorded the limitation but did not design a fix.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
