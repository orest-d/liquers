# Phase 3: Examples and Tests - Recipe-Provider Listing Contract

The primary scenario shows all three parts working together: a long-running server, a manifest
uploaded while it runs, and the HTTP and listing answers before and after. The tests then pin each
acceptance criterion separately.

## Overview Table

| Item | Kind | Demonstrates / checks | Criteria |
|---|---|---|---|
| Example 1 | scenario | listed vs producible, fresh manifest after upload, complete deep listing | A4, A6, B4, C3, C4 |
| Example 2 | scenario | a custom on-demand provider inherits correct defaults | A1, A2 |
| `on_demand_provider_splits_contains_and_can_make` | core unit | defaults for a provider listing nothing | A1, A2 |
| `default_provider_contains_equals_can_make` | core unit | `DefaultRecipeProvider` parity | A3 |
| `chain_forwards_contains_and_can_make` | core unit | chain forwarding | A5 |
| `can_make_is_true_if_any_provider_has_the_recipe` | core unit (renamed) | chain `can_make` over recipe-only mocks | A5 |
| `can_make_propagates_recipe_errors` | core unit | errors | A7 |
| `chain_forwards_directory_changed` | core unit | forwarding to every member | B8 |
| `default_directory_changed_is_a_no_op` | core unit | default | B8 |
| `manager_can_make_covers_unlisted_producible_keys` | core unit | manager split, `get_asset_info` | A6 |
| `listdir_keys_deep_contains_every_shallow_listing` | core unit, both managers | invariant | C1, C2, C5 |
| `keys_includes_root_level_recipes` | core unit, both managers | root | C3, C5 |
| `listdir_keys_deep_lists_only_what_contains_reports` | core unit | C4 against the manager | C4 |
| `removedir_unmaps_a_live_asset_of_the_directorys_own_recipe` | core unit | behaviour change | C6 |
| `can_make_answers_for_a_template_name_far_beyond_any_listing` | records unit (renamed) | manifest `can_make` | A4 |
| `contains_reports_only_explicit_chunks` | records unit | manifest `contains` | A4 |
| `a_manifest_added_after_listing_is_seen_after_directory_changed` | records unit | invalidation | B1 |
| `a_manifest_added_without_notice_is_not_seen` | records unit | freshness contract | decision |
| `a_removed_manifest_stops_resolving_after_directory_changed` | records unit | removal | B2 |
| `directory_changed_drops_the_subtree` | records unit | subtree | B3 |
| `clear_cache_drops_everything` | records unit | `clear_cache` | B5 |
| `an_unchanged_folder_is_listed_once` | records unit | cache works | B6 |
| `a_non_manifest_write_does_not_reparse_the_manifest` | records unit | no re-parse | B6 |
| `a_fill_racing_an_invalidation_is_not_kept` | records unit | race guard | B7 |
| `records_manifest_refresh` | lib integration | manager hook + chain + split end to end | A4, B1, B2 |
| axum asset API tests | axum integration | route, guards incl. websocket | A6 |
| axum store API test | axum integration | Store API notifies | B4 |
| axum deep listing test | axum integration | `?deep=true` includes the folder's own recipes | C1 |

## Example 1: A Manifest Uploaded to a Running Server

Store at start: `data/sales/daily.manifest.yaml` declaring explicit chunk `summary.csv` and template
`daily` (chunks `daily_0000.csv` …), plus `data/sales/recipes.yaml` declaring `report.txt`.

| Call | Before | After this design |
|---|---|---|
| `GET key/contains/data/sales/daily_0042.csv` | `true` | `false` (not listed) |
| `GET key/can_make/data/sales/daily_0042.csv` | — (no route) | `true` |
| `GET key/listdir/data/sales?deep=true` | omits `data/sales/report.txt` (the folder's own recipe) | contains every shallow listing: `daily.manifest.yaml`, `recipes.yaml`, `report.txt`, `summary.csv` under `data/sales/`; never a template chunk |
| `POST /api/store/data/data/sales/weekly.manifest.yaml`, then `GET key/can_make/data/sales/weekly_0001.csv` | `false` until restart | `true` at once |

The query `-R/data/sales/weekly_0001.csv` validates as a single `GetAsset` of that key.

## Example 2: A Custom On-Demand Provider

```rust
/// Produces `<name>.gen` in any folder; lists nothing.
struct PatternProvider;
#[async_trait]
impl<E: Environment> AsyncRecipeProvider<E> for PatternProvider {
    async fn has_recipes(&self, _key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> { Ok(false) }
    async fn assets_with_recipes(&self, _key: &Key, _envref: EnvRef<E>)
        -> Result<Vec<ResourceName>, Error> { Ok(vec![]) }
    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        Ok(key.filename().filter(|n| n.name.ends_with(".gen")).map(|_| Recipe::from(key)))
    }
    // recipe / recipe_plan delegate to recipe_opt; contains, can_make, directory_changed: defaults
}
```

`contains(a/x.gen) == false`, `can_make(a/x.gen) == true`, `AssetManager::get_asset_info(a/x.gen)`
is `Ok`. A provider author gets a correct "producible" answer without overriding anything.

## Corner Cases

- **Template chunk vs listing.** Never listed, always producible (A4, C4).
- **Key both stored and recipe-declared.** A file: `store.is_dir` is false, so no descent (C1).
- **Non-directory `key` in `listdir_keys_deep`.** `store.listdir` answers `Ok([])` on the memory and
  file stores, so the result is empty, as today.
- **Fill racing an invalidation.** Proven by construction in Phase 2 and pinned by a sequential
  simulation test (B7).
- **Out-of-band manifest edit.** Not seen until `clear_cache` — the decided contract, pinned by a
  test so a later change to it is deliberate.
- **Writes of non-manifest files into a manifest folder.** One extra `listdir`, no re-parse (B6).
- **Concurrency of `retain_async` with lookups.** `scc` map operations are individually atomic; a
  lookup either sees the entry or re-lists.
- **wasm32.** `AtomicU64` and `scc` are available; no spawn is added.

## Test Plan

### `liquers-core/src/recipes.rs` (`mod tests`, existing `TestEnv` / `MockProvider`)

Add `PatternProvider` (Example 2) and a `RecordingProvider` that pushes every `directory_changed`
argument into a `Mutex<Vec<Key>>`. Tests as in the overview. Rename
`contains_is_true_if_any_provider_has_the_recipe` to `can_make_is_true_if_any_provider_has_the_recipe`
and assert `can_make`: its `MockProvider`s declare recipes but list nothing, so `contains` is now
`false` for them, which the renamed test also asserts. `default_provider_contains_equals_can_make`
writes `data/recipes.yaml` (declaring `a.txt`, `b.txt`) with `store.set` and checks `data/a.txt`,
`data/b.txt`, `data/c.txt`, `other/a.txt`. `can_make_propagates_recipe_errors` writes a malformed
`recipes.yaml`.

### `liquers-core/src/assets.rs` (`mod tests`)

Fixture: `SimpleEnvironment<Value>` with `AsyncMemoryStore::new(&Key::new())` and the default
recipe provider; store `data/a.txt`, `data/sub/b.txt`, `data/recipes.yaml` declaring `top.txt`,
`data/sub/recipes.yaml` declaring `deep.txt`, and a root `recipes.yaml` declaring `root.txt`. Each
listing test runs once on `DefaultAssetManager` and once after installing `ImmediateAssetManager`
(a small generic helper, as the existing dual-manager tests do).

- `listdir_keys_deep_contains_every_shallow_listing`: for `data` and `data/sub`, every
  `listdir_keys(d)` key is in `listdir_keys_deep(data)`; no key outside `data/`; `data` absent;
  result sorted and unique.
- `keys_includes_root_level_recipes`: `keys()` contains `root.txt`.
- `listdir_keys_deep_lists_only_what_contains_reports`: every listed key satisfies
  `AssetManager::contains`.
- `removedir_unmaps_a_live_asset_of_the_directorys_own_recipe`: `get(data/top.txt)` (live),
  `removedir(data)`, then `lookup_key_asset(data/top.txt)` is `None`.
- `manager_can_make_covers_unlisted_producible_keys`: environment whose provider is
  `PatternProvider`.

### `liquers-records/src/provider.rs` (`mod tests`)

Add a test-only `CountingStore` wrapping `AsyncMemoryStore`: it delegates every `AsyncStore`
method and counts `listdir` and `get` calls in `AtomicUsize`s. (No counting store exists today;
`RefusingStore` in the same module shows the minimal `AsyncStore` impl shape.) Use the module's
`set_yaml` helper for manifests.

- `a_fill_racing_an_invalidation_is_not_kept`: make `manifest_names`' insert-and-recheck reachable as
  a private `cache_listing(folder, names, generation_before)` helper; call it with a generation
  read before a `directory_changed`; assert the next lookup lists again (counter +1).
- `a_non_manifest_write_does_not_reparse_the_manifest`: resolve a chunk (one `get` of the
  manifest), write `data/sales/other.csv`, call `directory_changed(data/sales)`, resolve again:
  `listdir` +1, `get` +0.
- The others as in the overview; `contains_reports_only_explicit_chunks` asserts `contains` true
  for `summary.csv` and false for `daily_0042.csv`.

### `liquers-lib/tests/records_manifest_refresh.rs` (`#![cfg(feature = "records")]`)

Environment from `LibKind` with the default provider chain and a memory store. Look up
`data/sales/weekly_0001.csv` (absent), write `weekly.manifest.yaml` through
`envref.get_asset_manager().set_binary(..)`, look up again: `AssetManager::can_make` is `true` for the
template chunk and `AssetManager::contains` is `true` for its explicit chunk. Remove it through
`AssetManager::remove`: both answer `false`.

### `liquers-axum/tests/`

Extend the assets API tests: `GET {b}/key/can_make/a/x.gen` → `true`, `GET {b}/key/contains/a/x.gen`
→ `false`, `submit` of `a/x.gen` succeeds, and a websocket subscribe to `a/x.gen` is accepted
(environment with `PatternProvider`). Extend the store API tests: upload a manifest after a first
lookup, then the new chunk resolves. Extend the deep-listing test: `?deep=true` on a folder with a
`recipes.yaml` includes that folder's own recipe keys.

### Commands

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-records --all-features --lib --tests
cargo test -p liquers-records --lib --tests
cargo test -p liquers-lib --test records_manifest_refresh
cargo test -p liquers-axum
```

## Documentation and Learning Log

- Guide-worthy: "uploading a manifest to a running server" (Example 1) and "writing an on-demand
  provider" (Example 2) — the snippets above, linked to `records_manifest_refresh.rs` and
  `on_demand_provider_splits_contains_and_can_make` as the executable versions.
- Learning recorded for Phase 5: an overloaded method answered two questions and every caller
  wanted the one its default did not answer; caches that are version-checked per hit need no
  invalidation, only listings do.
