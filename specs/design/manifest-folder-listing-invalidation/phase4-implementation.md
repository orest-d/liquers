# Phase 4: Implementation Plan - Folder-Cache Invalidation for `ManifestRecipeProvider`

0. **Decision gate.** Confirm Q1 (event-driven contract) and Q2 (Store API hook). If a TTL is
   chosen, revise Phase 2 first; if Q2 is declined, skip step 5 and document Store API writes as
   out-of-band in step 6.
1. **Trait hook.** `liquers-core/src/recipes.rs`: add `directory_changed` with a no-op default;
   forward it in `RecipeProviderChain`. Add `chain_forwards_directory_changed`.
   Proof: `cargo test -p liquers-core --lib recipes::`; `cargo check -p liquers-core --target
   wasm32-unknown-unknown`.
2. **Manager call.** `liquers-core/src/assets.rs`, `AssetManager::refresh_listing_version`:
   call the hook first, before the early return; update its doc. Depends on 1.
   Proof: `cargo test -p liquers-core --lib --tests` (no behaviour change for core providers).
3. **Provider.** `liquers-records/src/provider.rs`: `generation` field, guarded insert in
   `manifest_names`, `directory_changed` (subtree `retain_async` on both maps), `clear_cache`,
   doc comment rewrite. Depends on 1. Add the seven Phase 3 tests.
   Proof: `cargo test -p liquers-records --all-features --lib --tests` and
   `cargo test -p liquers-records --lib --tests`.
4. **Lib integration.** Add `liquers-lib/tests/records_manifest_refresh.rs` (gated on
   `records`). Depends on 2-3. Proof: `cargo test -p liquers-lib --test records_manifest_refresh`;
   `cargo test -p liquers-lib --no-default-features --lib --tests` still compiles (file gated).
5. **Store API.** `liquers-axum/src/store/handlers.rs`: private `notify_directory_changed`
   helper; call after each successful write/delete. Extend the store API integration test.
   Proof: `cargo test -p liquers-axum`.
6. **Docs and records.** Freshness contract in `reference/RECORD_STREAMS.md` and
   `guides/RECORD_STREAM_GUIDE.md` (where `ManifestRecipeProvider` is described); hook in
   `reference/api/DOC_08_RECIPES_PLANS.md`. History rows + `reviewed:`. Close the issue with the
   decision recorded. Regenerate and check the docs index.
7. **Checks.** `cargo fmt`; `bash scripts/check-build-matrix.sh` (records feature split and wasm
   rows — the provider is built for wasm via `liquers-lib`); diff review for: hook called before
   the early return, no `Result` swallowed silently in write paths, no `println!`, no `_ =>`.

## Final Review

Executable once Q1/Q2 are confirmed. Steps 1-2 are safe alone (no-op default); step 3 is the
behaviour change; step 5 is separable. Rollback per step.
