# Phase 4: Implementation Plan - Deep Listing Includes Recipe Keys at Every Level

1. **Tests first.** Add the four Phase 3 tests to `liquers-core/src/assets.rs` `mod tests`.
   Run `cargo test -p liquers-core --lib listdir_keys_deep keys_includes removedir_unmaps` —
   the first, second and fourth must fail at HEAD (proving the bug); the third fails too.
2. **Rewrite the trait default.** `trait AssetManager<E>`, `listdir_keys_deep` (≈5300): the
   Phase 2 walk, with a doc comment stating "contains `listdir_keys(d)` for every directory `d`
   under `key`, recipe-declared keys included; never `key` itself". Depends on 1.
   Proof: step-1 tests pass for the `ImmediateAssetManager` variant.
3. **Delete the override.** `impl AssetManager<E> for DefaultAssetManager<E>`,
   `listdir_keys_deep` (≈7517): remove it. Depends on 2. Proof: step-1 tests pass for both
   managers. Containment: restoring the override restores old behaviour for the default manager.
4. **Regression sweep.** `cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum`
   (deep listing endpoint). If an existing test asserted the incomplete listing, correct its
   expectation and note it in the commit message.
5. **Docs and records.** Review `reference/ASSETS.md` and
   `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` for `listdir_keys_deep`/`keys()`
   statements; add the invariant where described (History row + `reviewed:`). Close
   `specs/issues/ASSET-MANAGER-LISTDIR-KEYS-DEEP-OMITS-TOP-LEVEL-RECIPES.md`. Regenerate and
   check the docs index.
6. **Checks.** `cargo fmt`, `cargo clippy -p liquers-core`, `cargo test -p liquers-lib --lib
   --tests` (lib environments use the same managers). Diff review: no change to `listdir`,
   providers or store code; no `println!`; no `_ =>` arms.

## Final Review

Phases agree. The removedir effect is the one externally visible side effect and has its own
test. Rollback is confined to the method body and the deleted override.

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes. The trait default (`assets.rs` ≈5300) and the identical
  `DefaultAssetManager` override (≈7517) still add recipe names only for stored
  *subdirectories*.
- **Solution correct:** yes for criteria 1-4 and 6. Building the deep listing on `listdir`
  makes the invariant hold by construction, and deleting the duplicate override is good.
- **Possibly unnecessary:** criterion 5 (descending into recipe-only directories). No provider in
  the repository can produce that case; Phase 1 says so itself. The price is one `has_recipes`
  call for every recipe name that is not stored, at every level. With the lib provider chain that
  is a `store.contains` plus a manifest `listdir` per name. Recommend dropping criterion 5, or
  keeping it with the cost stated. If it stays, `removedir` will pass a recipe-only *directory*
  key to `remove` (the key is not `store.is_dir`), and no test covers that.
- **Detail:** sufficient.
- **Tests:** good. Add one records-level assertion: a manifest *template* chunk does **not**
  appear in the deep listing, while explicit chunks do. That pins "listed" to the same meaning
  `recipe-contains-addressability` gives it.
- **Interactions:** strong, with `recipe-contains-addressability`, which defines "listed"
  (`AssetManager::contains` = stored or listed). This design should state the invariant "every
  key in `listdir_keys_deep` satisfies `AssetManager::contains`". It also interacts with
  `manifest-folder-listing-invalidation`: a stale provider listing makes a deep listing stale.
  Both touch the same `AssetManager` default block (≈5216-5320).
- **Verdict:** ready after the criterion-5 decision. Implement after
  `recipe-contains-addressability` (same PR or next).
