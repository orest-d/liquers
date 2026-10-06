# Phase 5: Documentation - Recipe-Provider Listing Contract

**Status: executed 2026-10-06.** Written with Phase 4 on 2026-10-05 and executed after implementation, following
`.claude/skills/liquers-project/references/phase5-documentation.md`. The sections the skill
requires after implementation are present and marked *pending*.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4 steps 1-16)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

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

Implemented as planned, in the order A, B, C.

- **A.** `AsyncRecipeProvider::can_make` (default `recipe_opt(..).is_some()`); `contains` keeps its
  listing meaning; the chain's `contains` is any member's `contains`, its `can_make` the old
  `recipe_opt` test. `ManifestRecipeProvider`'s `contains` override is deleted.
  `AssetManager::can_make` is new; `get_asset_info`, axum `key_metadata`/`submit_key` and the
  websocket subscribe guards use it; new route `GET key/can_make/{*key}`.
- **B.** `AsyncRecipeProvider::directory_changed` (no-op default, forwarded by the chain), called as
  the first statement of `refresh_listing_version`; the Store API write/remove/makedir/removedir
  handlers call that method through a private `refresh_after_write`. `ManifestRecipeProvider` drops
  folder listings only, guarded by an atomic `generation` (insert, then re-check); `clear_cache()`.
- **C.** `listdir_keys_deep` rebuilt on `listdir` over store directories; `DefaultAssetManager`'s
  duplicate `contains`, `keys`, `listdir`, `listdir_keys`, `listdir_keys_deep` overrides deleted;
  `removedir` therefore unmaps the directory's own recipe assets with no further change.

## Documentation Delivered

The seven `affects_docs` documents were updated as planned (each with a History row and a
`reviewed:` bump); the three source issues are `closed` with resolutions; `specs/README.md` marks
the capability `built`. No new reference or guide.

## Issues Filed

None.

## Important Learning

- An overloaded method answered two questions and every caller wanted the one its default did not
  answer; splitting it removed the need for a manifest override.
- Caches that are version-checked per hit need no invalidation; only listings do.
- **Deviation from the Phase 1/3 scenario:** a *template* chunk of a manifest uploaded to a running
  server was never hidden by the folder cache — the template fast path names its manifest by
  prefix and does not read the listing (and `ManifestRecipeProvider` was `can_make`-true for it
  before this design). The stale listing hid *explicit* chunks and non-canonical names. The tests
  and the reference text use explicit chunks for the freshness behaviour accordingly.
- Counting `get` on a manifest provider's store must count manifest keys only: the
  `recipes.yaml` collision check also reads the store.

## Conformance and Remaining Work

All acceptance criteria A1-A7, B1-B8 and C1-C6 are covered by tests (see below). Not implemented
by decision: a websocket-level subscribe test (the guards are the same `can_make` call as
`submit_key`, covered by `rplc02`); `default_provider_contains_equals_can_make` (parity is
exercised by the deep-listing tests, which require every listed key to satisfy
`AssetManager::contains`).

## Validation

`cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-records --all-features --lib --tests`,
`cargo test -p liquers-records --lib --tests`, `cargo test -p liquers-axum`,
`cargo test -p liquers-lib --test records_manifest_refresh`;
`python3 scripts/docs_index.py --check`.

Not run: `scripts/check-build-matrix.sh` and the wasm32 rows (the wasm32 target is not installed in
this environment); `cargo test -p liquers-lib --lib --tests` and the commands above passed.
