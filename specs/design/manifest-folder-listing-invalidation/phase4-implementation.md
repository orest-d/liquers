# Phase 4: Implementation Plan - Folder-Cache Invalidation for `ManifestRecipeProvider`

Freshness contract and Store API hook were decided on 2026-10-04 (event-driven; Store API
notifies); no decision gate remains.

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
   `reference/api/DOC_08_RECIPES_PLANS.md`. History rows + `reviewed:`. Close the issue, recording the
   decision. Regenerate and check the docs index.
7. **Checks.** `cargo fmt`; `bash scripts/check-build-matrix.sh` (records feature split and wasm
   rows — the provider is built for wasm via `liquers-lib`); diff review for: hook called before
   the early return, no `Result` swallowed silently in write paths, no `println!`, no `_ =>`.

## Final Review

Executable as written. Steps 1-2 are safe alone (no-op default); step 3 is the
behaviour change; step 5 is separable. Rollback per step.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. `folders` is filled once per folder and never invalidated
  (`liquers-records/src/provider.rs`, type doc ≈61-66).
- **Solution correct:** mostly. Two defects:
  1. **The race guard as written does not close the race.** "Read `generation` before `listdir`;
     insert only if unchanged" is a check followed by an insert, and an invalidation can land
     between the two. Insert first, then re-read `generation`; if it moved, remove the entry
     just inserted (`remove_if` on the same `Arc`). Alternatively, do the check and the insert
     under the `entry_async` guard.
  2. **The invalidation is too broad for the record-stream workload.** Chunks are stored in the
     manifest's own folder (`cwd` = folder), so every materialized chunk write calls
     `refresh_listing_version(folder)`. That drops the folder listing **and** every parsed
     manifest in it, so the next chunk lookup re-lists and re-parses. The `manifests` cache is
     already version-checked on every lookup, so dropping it is redundant. Drop only `folders`.
     Better still, pass the changed key (or a "manifest file changed" bit) so writes of names
     that do not end in `.manifest.yaml` skip the invalidation.
- **Unnecessary:** the `manifests` subtree drop (see 2). `clear_cache` is justified by the
  decided freshness contract.
- **Detail:** sufficient apart from the above.
- **Tests:** good. Add one test: writing a non-manifest file into a folder does not force the
  manifest to be re-parsed (a `get` counter on the counting store). It guards defect 2.
- **Interactions:** edits the same trait, `RecipeProviderChain` impl, `ManifestRecipeProvider`
  and axum crate as `recipe-contains-addressability`, and its own lib test already refers to
  that design's `can_make`. The hook sits in `refresh_listing_version`, which `save_to_store`
  calls, so it also touches `save-to-store-skip-outcome` (compatible: the hook fires only on
  `Written`). `listdir-keys-deep-recipe-union` reads the listings this cache feeds.
- **Verdict:** needs the two fixes above. Recommend implementing it **together with
  `recipe-contains-addressability`** (one PR that changes the recipe-provider contract).
