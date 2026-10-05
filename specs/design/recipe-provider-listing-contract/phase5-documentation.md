# Phase 5: Documentation - Recipe-Provider Listing Contract

**Status: plan.** Written with Phase 4 on 2026-10-05 and executed after implementation, following
`.claude/skills/liquers-project/references/phase5-documentation.md`. The sections the skill
requires after implementation are present and marked *pending*.

## Completion Preconditions

- [ ] Implementation is finished and validated (Phase 4 steps 1-16)
- [ ] All user comments are answered or incorporated
- [ ] All review comments are answered or incorporated
- [ ] Documentation is consistent with the implemented and tested behavior
- [ ] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None. Every fact has an existing owner (see below).

### New Guide Documents

None. The two repeatable tasks — uploading a manifest to a running server, writing an on-demand
provider — extend existing guides.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/api/DOC_08_RECIPES_PLANS.md` | provider contract: `contains` (shown) vs `can_make` (produced), `can_make ⊇ contains`, `directory_changed`; chain table (≈166) gets a `can_make` row and a corrected `contains` row; the manifest-override paragraph (≈177) is rewritten |
| `reference/ASSETS.md` | `AssetManager::contains` vs `can_make`; `listdir_keys_deep` / `keys()` invariant; `removedir` unmaps the directory's own recipe assets |
| `reference/RECORD_STREAMS.md` | caches paragraph (≈364-368): event-driven folder listing, Store API writes included, `clear_cache` for out-of-band edits; template chunks are producible, not listed; drop the link to the closed issue |
| `guides/RECORD_STREAM_GUIDE.md` | how to add a manifest to a running server; when to call `clear_cache` |
| `reference/WEB_API_SPECIFICATION.md` | new `key/can_make` row; `key/contains` = stored or listed; remove the deep-listing caveat (≈294) |
| `guides/WEB_API_GUIDE.md` | `key/can_make` example beside `key/contains` (≈280) |
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | a manager calls `refresh_listing_version` after every write; it now also notifies the recipe provider |

Each gets a `## History` row (source `phase-5`) and a `reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By shared area (`core/assets`, `records`, `axum`): `ASSET_LIFECYCLE.md`, `ASSET_SET_OPERATION.md`,
`DEPENDENCIES_STATUS.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`, `DOC_01_ARCHITECTURE_REFERENCE.md`,
`ENVIRONMENT_CONFIG.md`, `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, `LANGUAGE-INTEGRATION_GUIDE.md`,
`PROJECT_OVERVIEW.md`, `UNITTEST_GUIDE.md` — none states what `contains`, deep listings or the
manifest cache do (checked by search on 2026-10-05). Re-check at execution with
`grep -rn "contains\|listdir_keys_deep\|ManifestRecipeProvider" specs/reference specs/guides`.

### Links and Capability Map

`specs/README.md`: the recipe/record-streams entry links this design while it is in progress, and
the updated references once it is complete.

### Issues to Close

`RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY`, `MANIFEST-PROVIDER-FOLDER-LISTING-NEVER-REFRESHES`,
`ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES` → `status: closed`, each with a resolution
naming its tests and recording the decisions of Phase 1.

## Implementation Summary

*Pending — written after implementation.*

## Documentation Delivered

*Pending.*

## Issues Filed

*Pending.* Candidate known now: none.

## Important Learning

*Pending.* Seed from Phase 3's learning log.

## Conformance and Remaining Work

*Pending.*

## Validation

*Pending.* Planned: `python3 scripts/docs_index.py --check`; every planned document reviewed against
the code; `affects_docs` matches the documents actually reviewed.
