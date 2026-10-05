# Phase 4: Implementation Plan - `contains` (Listed) and `can_make` (Producible)

The provider contract was decided on 2026-10-04 and the mirrored asset-manager/HTTP split on
2026-10-05; no decision gate remains.

1. **Trait.** `liquers-core/src/recipes.rs`: add `can_make` with the `recipe_opt` default; rewrite
   `contains`' doc ("what its directory shows"); add the trait-level paragraph. Add
   `on_demand_provider_splits_contains_and_can_make`, `default_provider_contains_equals_can_make`,
   `can_make_propagates_recipe_errors`. Proof: `cargo test -p liquers-core --lib recipes::`.
2. **Chain.** Same file, `RecipeProviderChain`: move today's `contains` body to `can_make`; new
   `contains` = any member's `contains`. Add `chain_forwards_contains_and_can_make`; re-run and,
   if needed, adjust `contains_is_true_if_any_provider_has_the_recipe`. Depends on 1.
3. **Manifest provider.** `liquers-records/src/provider.rs`: rename the `contains` override to
   `can_make` (doc comment updated); rely on the default `contains`. Rename/extend the test and add
   `contains_reports_only_explicit_chunks`. Depends on 1.
   Proof: `cargo test -p liquers-records --all-features --lib --tests`.
4. **Describe paths.** `liquers-core/src/assets.rs` `get_asset_info` (≈5206) and
   `liquers-axum/src/assets/common.rs` (≈253): `contains` → `can_make`. Depends on 1. Without this
   step, step 3 would make template chunks undescribable.
   Proof: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-axum`.
5. **Manager `can_make`.** `trait AssetManager<E>`: add `can_make` (Phase 2 body), document
   `contains` as "stored or listed". Add `manager_can_make_covers_unlisted_producible_keys`.
   Depends on 1.
6. **Axum.** `assets/common.rs` `submit_key` and `assets/websocket.rs` subscribe guards →
   `manager.can_make`; new `key_can_make_handler` and route `{b}/key/can_make/{*key}` in
   `assets/builder.rs`; extend the integration tests. Depends on 5. Proof: `cargo test -p liquers-axum`.
7. **Docs and records.** `reference/api/DOC_08_RECIPES_PLANS.md` (both provider methods, subset
   rule), `reference/ASSETS.md` (manager methods), the axum assets API reference (new route).
   History rows + `reviewed:`. Close `specs/issues/RECIPE-CONTAINS-DEFAULT-ASSUMES-ENUMERABILITY.md`
   recording the decision. Regenerate and check the docs index.
8. **Checks.** `cargo fmt`; `cargo test -p liquers-core --lib --tests`;
   `cargo test -p liquers-lib --lib --tests`; `cargo check -p liquers-core --target
   wasm32-unknown-unknown`; `bash scripts/check-build-matrix.sh` (records feature split). Diff
   review: no change to `assets_with_recipes`/`has_recipes`; every former "producible" caller now
   uses `can_make`; no `println!`.

## Final Review

Executable as written. Steps 1-4 fix the defect at the provider level; steps 5-6 add the
mirrored manager method and HTTP route. Rollback per step; `can_make`'s default makes step 1 harmless on its own.
