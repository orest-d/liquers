---
id: RECIPE-PROVIDER-LISTING-CONTRACT
kind: design
title: Recipe-provider listing contract — listed vs producible keys, folder-cache invalidation, complete deep listings
workflow: liquers-project
status: in_review
phase: documentation
readiness: ready
area: [core/assets, records, axum]
issues: [RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY, MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES, ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES]
merged: 2026-10-05
affects_docs: [reference/api/DOC_08_RECIPES_PLANS.md, reference/ASSETS.md, reference/RECORD_STREAMS.md, guides/RECORD_STREAM_GUIDE.md, reference/WEB_API_SPECIFICATION.md, guides/WEB_API_GUIDE.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-05
---
# recipe-provider-listing-contract Design Tracking

Merged on 2026-10-05, by maintainer decision after the post-Phase-4 review, from three designs whose
implementations depend on each other. Each is now `superseded` by this one:

| Former design | Source issue (now owned here) | Became |
|---|---|---|
| `recipe-contains-addressability` | `RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY` (leading) | Part A |
| `manifest-folder-listing-invalidation` | `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES` | Part B |
| `listdir-keys-deep-recipe-union` | `ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES` | Part C |

Why one design: all three change the `AsyncRecipeProvider` contract or its users, all three touch
`RecipeProviderChain`, `ManifestRecipeProvider` and the asset manager's listing methods, Part B's
tests rely on Part A's `can_make`, and Part C defines its result in terms of Part A's "listed".

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan
- [x] Phase 5: Documentation (written 2026-10-06; awaiting approval)

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
