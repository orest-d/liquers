---
id: RECIPE-ONLY-FOLDER-NAMES-HIDE-A-CREATED-FOLDER-FROM-ITS-PARENT
kind: issue
title: A recipe provider that lists names in a folder with no stored file can leave the parent listing stale after a write
status: draft
priority: P3
complexity: S
area: [core/assets]
design: recipe-folder-names-listing-contract
created: 2026-10-18
github:
---

## Problem

`AssetManager::refresh_listing_version` (`liquers-core/src/assets.rs`) decides whether a write
can have changed the parent's membership from the *merged* listing of the written folder
(`AssetManager::listdir`: store entries plus `AsyncRecipeProvider::assets_with_recipes`): it climbs
to the parent only when that listing was or became empty. The in-tree providers
(`DefaultRecipeProvider`, `ManifestRecipeProvider`, `RecipeProviderChain` of them) list names in a
folder only from a file stored in that same folder (`recipes.yaml`, `*.manifest.yaml`), so the
merged listing is empty exactly when the stored one is. A custom provider that lists names in a
folder which has no stored file (generated recipes) breaks that: a write that physically creates the
folder sees a non-empty merged listing before and after, stops, and a registered listing of the
parent misses the new folder until something else changes it. The removal that empties the
physical folder is the mirror case. `AsyncRecipeProvider::assets_with_recipes` states no contract
either way.

## Impact

None with the in-tree providers. A custom generating provider gets a stale parent listing (an
index, `ns-search/catalog`) after such a write; an audit or any later membership change of the
parent repairs it.

## Expected behaviour

Either state the contract on `assets_with_recipes` (names in a folder imply a stored file there),
or decide the climb from the stored listing's emptiness rather than the merged one's, at the cost
of knowing the stored listing before the write. Deciding it from "did the parent's listing move"
reads the parent on every ordinary write, which for a recursive catalog is one listing per level.

## Discovery

Codex review on PR #160 (`design/new-folder-write-ancestor-listing-refresh`), 2026-10-18; traced
against the in-tree providers, pinned by `removing_a_recipe_folder_expires_the_parent_index`.

## Reproduction at HEAD (bulk design, 2026-10-20)

Persists at 18732dd: `refresh_listing_version` / `refresh_one_listing` (`liquers-core/src/assets.rs`)
still decide the climb from the merged `AssetManager::listdir`, and the trait doc of
`AsyncRecipeProvider::assets_with_recipes` still states no contract. Origin design
`new-folder-write-ancestor-listing-refresh` is complete. Designed in
`design/recipe-folder-names-listing-contract/` (recommended: state the contract; the stored-listing
climb is the alternative).
