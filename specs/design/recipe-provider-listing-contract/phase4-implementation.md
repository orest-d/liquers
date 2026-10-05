# Phase 4: Implementation Plan - Recipe-Provider Listing Contract

## Overview

One implementation PR, in three parts, in this order: A (listed vs producible), B (invalidation),
C (deep listing). Each part ends with green tests, so a part can be reverted alone. All contract
decisions are recorded in Phase 1; no decision gate remains.

## Implementation Steps

**Part A — listed vs producible**

1. **Provider trait.** `liquers-core/src/recipes.rs`: add `can_make` with the `recipe_opt` default;
   rewrite `contains`' doc; add the trait-level paragraph. Add `PatternProvider` and the tests
   `on_demand_provider_splits_contains_and_can_make`, `default_provider_contains_equals_can_make`,
   `can_make_propagates_recipe_errors`.
   Proof: `cargo test -p liquers-core --lib recipes::`.
2. **Chain.** Same file: move today's `RecipeProviderChain::contains` body to `can_make`; new
   `contains` = any member's `contains`. Add `chain_forwards_contains_and_can_make`; rename
   `contains_is_true_if_any_provider_has_the_recipe` to `can_make_…` (Phase 3). Depends on 1.
3. **Manifest provider.** `liquers-records/src/provider.rs`: **delete** the `contains` override and
   update the comments that cite it. Rename `contains_answers_for_a_template_name_far_beyond_any_listing`
   to `can_make_…`, add `!contains` for the same key, add `contains_reports_only_explicit_chunks`.
   Depends on 1. Proof: `cargo test -p liquers-records --all-features --lib --tests`.
4. **Describe paths.** `liquers-core/src/assets.rs` `get_asset_info` (≈5206) and
   `liquers-axum/src/assets/common.rs` (≈253): `contains` → `can_make`. Depends on 1. Without this
   step, step 3 makes template chunks undescribable.
5. **Manager `can_make`.** `trait AssetManager<E>`: add `can_make`; document `contains`; delete
   `DefaultAssetManager`'s identical `contains` override (≈7472). Add
   `manager_can_make_covers_unlisted_producible_keys`. Depends on 1.
6. **Axum.** `submit_key` and the websocket subscribe guards → `manager.can_make`; add
   `key_can_make_handler` and the route `{b}/key/can_make/{*key}`; extend the asset API tests,
   including the websocket subscribe case. Depends on 5. Proof: `cargo test -p liquers-axum`.

**Part B — folder-cache invalidation**

7. **Hook.** `recipes.rs`: add `directory_changed` (no-op default); forward it in
   `RecipeProviderChain`. Add `RecordingProvider`, `chain_forwards_directory_changed`,
   `default_directory_changed_is_a_no_op`. Proof: `cargo check -p liquers-core --target
   wasm32-unknown-unknown`.
8. **Manager call.** `assets.rs` `refresh_listing_version`: call
   `self.get_recipe_provider().directory_changed(dir).await` as its **first** statement, before the
   early return; update its doc. Depends on 7. Proof: `cargo test -p liquers-core --lib --tests`.
9. **Provider.** `provider.rs`: `generation` field (initialised in `new` and by `Default`);
   `manifest_names` reads the generation before `listdir`, inserts through a private
   `cache_listing(folder, names, generation_before)` that re-checks and removes its own `Arc` if
   the generation moved; `directory_changed` (bump, then `retain_async` on `folders` only);
   `clear_cache`; rewrite the type's doc comment ("for the life of the provider" → the
   event-driven contract). Add `CountingStore` and the eight Part B records tests. Depends on 7.
   Proof: `cargo test -p liquers-records --all-features --lib --tests` and
   `cargo test -p liquers-records --lib --tests`.
10. **Lib integration.** `liquers-lib/tests/records_manifest_refresh.rs`. Depends on 5, 8, 9.
    Proof: `cargo test -p liquers-lib --test records_manifest_refresh`;
    `cargo test -p liquers-lib --no-default-features --lib --tests` still compiles (file gated).
11. **Store API.** `liquers-axum/src/store/handlers.rs`: private
    `refresh_after_write(env, key)`; call it after each successful write or removal listed in
    Phase 2. Extend the store API test. Depends on 8. Proof: `cargo test -p liquers-axum`.

**Part C — complete deep listings**

12. **Failing tests.** Add the four Part C tests to `assets.rs` `mod tests`; confirm that
    `listdir_keys_deep_contains_every_shallow_listing`, `keys_includes_root_level_recipes` and
    `removedir_unmaps_a_live_asset_of_the_directorys_own_recipe` fail at this point.
13. **Rewrite the default.** `trait AssetManager<E>::listdir_keys_deep` (≈5300): the Phase 2 loop
    with its doc comment.
14. **One implementation.** In `impl AssetManager<E> for DefaultAssetManager<E>`, compare the
    bodies of `keys`, `listdir`, `listdir_keys` (≈7482-7515) with the trait defaults; delete each
    that is identical, and delete the `listdir_keys_deep` override (≈7517) in any case. Proof: the
    step-12 tests pass on both managers.
15. **Regression sweep.** `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-axum`
    (deep listing and `removedir` endpoints), `cargo test -p liquers-lib --lib --tests`. If an
    existing test asserted the old, incomplete listing, correct it and say so in the commit message.

**Close-out**

16. **Checks.** `cargo fmt`; `cargo clippy -p liquers-core -p liquers-records`;
    `bash scripts/check-build-matrix.sh` (records feature split and wasm rows). Diff review:
    `refresh_listing_version` calls the hook before its early return; no `_ =>` arm; no `println!`;
    no change to `assets_with_recipes` / `has_recipes`; every former "producible" caller uses
    `can_make`.
17. **Phase 5.** Execute `phase5-documentation.md`.

## Testing Plan

Unit tests run per step as listed. Integration tests run at steps 6, 10, 11 and 15. The full matrix
runs at step 16. Order of proof: each part's failing tests are written before its code (steps 12-13
explicitly; in A and B the renamed tests fail until the method exists).

## Agent Assignment

| Steps | Tier | Skills | Knowledge |
|---|---|---|---|
| 1-6 | Sonnet | rust-best-practices, liquers-unittest | Phases 1-3; `recipes.rs` trait and chain; `assets.rs` ≈5200-5320; axum `assets/` |
| 7-11 | Sonnet | rust-best-practices, liquers-unittest | Phase 2 race-guard argument; `provider.rs`; `scc` 3.x map API; axum `store/handlers.rs` |
| 12-15 | Sonnet | rust-best-practices, liquers-unittest | Phase 2 deep-listing loop; `removedir`; dual-manager test helpers in `assets.rs` |
| 16 | Haiku | — | CLAUDE.md "Building and testing" |
| Review of the whole diff | Opus | rust-best-practices | all phases |

## Rollback Plan

- Part C: restore the old trait default and the deleted overrides.
- Part B: remove the hook call from `refresh_listing_version` and the Store API helper; the
  provider's `directory_changed` then never runs, and the cache behaves as before.
- Part A: restore the manifest and chain `contains` bodies and the callers' `contains`; `can_make`
  can stay (harmless default) or be removed with the route.

## Phase 5 Entry Criteria

- All steps 1-16 complete; every test listed in Phase 3 exists and passes.
- `bash scripts/check-build-matrix.sh` passes.
- Review comments on the PR answered or incorporated.

## Post-Phase-4 Review Resolution (2026-10-05)

The findings of the review recorded in the three superseded designs are incorporated here:
the manifest provider's `contains` override is deleted rather than renamed; the folder-cache race
guard is insert-then-recheck; invalidation drops folder listings only; descent into recipe-only
directories is dropped; a non-manifest-write test, a websocket subscribe test and a "deep listing
lists only what `contains` reports" test were added; `DefaultAssetManager`'s duplicate overrides go.
