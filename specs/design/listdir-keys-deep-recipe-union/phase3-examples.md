# Phase 3: Examples and Tests - Deep Listing Includes Recipe Keys at Every Level

## Example

Store: `data/a.txt`, `data/sub/b.txt`, `data/recipes.yaml` declaring `top.txt`,
`data/sub/recipes.yaml` declaring `deep.txt`.

| Call | Before | After |
|---|---|---|
| `listdir_keys(data)` | `a.txt, recipes.yaml, sub, top.txt` | same |
| `listdir_keys_deep(data)` | `a.txt, recipes.yaml, sub, sub/b.txt, sub/deep.txt, sub/recipes.yaml` | adds `data/top.txt` |
| `keys()` with `recipes.yaml` at root declaring `root.txt` | no `root.txt` | includes `root.txt` |

## Tests to Add (`liquers-core/src/assets.rs`, `mod tests`)

Fixture: `SimpleEnvironment<Value>` with `AsyncMemoryStore::new(&Key::new())`, the default
recipe provider, and a `recipes.yaml` written with `store.set` — follow the existing
recipe-listing tests in this module (search `assets_with_recipes` / `recipes.yaml` in tests) for
the exact `recipes.yaml` body and the `hello`-style command registration. Each test runs against
both managers (a small generic helper taking the environment, called once with the default
manager and once after installing `ImmediateAssetManager`, as other dual-manager tests do).

| Test | Asserts | Criterion |
|---|---|---|
| `listdir_keys_deep_contains_every_shallow_listing` | for `data` and `data/sub`, every `listdir_keys(d)` key ∈ `listdir_keys_deep(data)`; no key outside `data/`; `data` itself absent | 1, 2, 3, 6 |
| `keys_includes_root_level_recipes` | `keys()` contains `root.txt` | 4, 6 |
| `listdir_keys_deep_descends_into_recipe_only_directory` | a test provider (as `MockProvider` in `recipes.rs` tests) reporting `virt` as a name of `data` with `has_recipes(data/virt) == true` and `assets_with_recipes(data/virt) == [x.txt]` → `data/virt/x.txt` listed | 5 |
| `removedir_unmaps_a_live_asset_of_the_directorys_own_recipe` | `get(data/top.txt)` (evaluated, live), `removedir(data)`, then `lookup_key_asset(data/top.txt)` is `None` | Phase 1 removedir note |

## Existing Tests to Run

All `removedir_*` tests in `assets.rs`; `liquers-axum` tests covering `listdir` with
`deep=true` (`cargo test -p liquers-axum`).

## Coverage Review

All criteria and the removedir behaviour change are covered; the recipe-only-directory branch has
a dedicated provider fixture because the default provider cannot produce that case.
