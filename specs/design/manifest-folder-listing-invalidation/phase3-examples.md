# Phase 3: Examples and Tests - Folder-Cache Invalidation for `ManifestRecipeProvider`

## Scenario

A server serves `data/sales/daily.manifest.yaml`. An operator uploads
`data/sales/weekly.manifest.yaml` (through the asset manager or `POST /api/store/data/...`).
Before: `-R/data/sales/weekly_0001.csv` answers "no recipe" until restart. After: it resolves on
the next request.

## Tests to Add

### `liquers-records/src/provider.rs` (`mod tests`)

Reuse the module's existing fixture (store, manifest YAML helpers, and the counting store used
for "Lookup cost" assertions — follow
`contains_answers_for_a_template_name_far_beyond_any_listing` and the explicit-chunk tests for
the exact setup).

| Test | Steps | Asserts | Criterion |
|---|---|---|---|
| `a_manifest_added_after_listing_is_seen_after_directory_changed` | list `data/sales` (caches), `store.set` a second manifest, call `provider.directory_changed(&data/sales)` | its explicit chunk is in `assets_with_recipes`; `contains` true | 1 |
| `a_manifest_added_without_notice_is_not_seen` | same without the call | still absent — documents the contract | freshness contract |
| `a_removed_manifest_stops_resolving_after_directory_changed` | cache, `store.remove` manifest, notify | `recipe_opt` of its chunk `None`; `manifests` no longer holds it | 2 |
| `directory_changed_drops_the_subtree` | cache `data` and `data/sales`, notify `data` | both listings re-read (counting store sees two `listdir`) | 3 |
| `clear_cache_drops_everything` | cache two folders, `clear_cache()` | both re-read | 5 |
| `an_unchanged_folder_is_listed_once` | two lookups, no notice | one `listdir` | 6 |
| `a_fill_racing_an_invalidation_is_not_cached` | read generation, invalidate, then complete a fill with the stale generation (call the internal insert path directly) | next lookup re-lists | Phase 1 race finding |

### `liquers-lib/tests/` — new `records_manifest_refresh.rs` (`#![cfg(feature = "records")]`)

Environment from `LibKind` with the default provider chain (which carries
`ManifestRecipeProvider`) and a memory store. Write a manifest via
`envref.get_asset_manager().set(...)` (or `set_state`) after a first lookup; assert the new chunk
resolves through `AssetManager::contains` — proves the manager hook and chain forwarding
(criteria 1, 7).

### `liquers-axum/tests/` — extend the store API integration tests

`POST /api/store/data/data/sales/weekly.manifest.yaml` after a first recipe lookup, then
`GET /api/assets/...` (or `recipe_opt` via the env) resolves the new chunk (criterion 4).

### `liquers-core/src/recipes.rs` (`mod tests`)

`chain_forwards_directory_changed`: two recording test providers in a `RecipeProviderChain`;
both see the call (criterion 7 forwarding). `default_directory_changed_is_a_no_op` compiles
and returns for `TrivialRecipeProvider`.

## Queries

`-R/data/sales/weekly_0001.csv` — a resource key; `-R/` consumes the rest as the key (validated
form; no action segment intended).

## Coverage Review

Every criterion and the race finding has a test; the "not seen without notice" test pins the
decided contract.
