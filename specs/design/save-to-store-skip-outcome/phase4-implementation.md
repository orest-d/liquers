# Phase 4: Implementation Plan - Skipped Store Writes Are Not Persists

1. **Failing test.** Add `cancelled_asset_save_is_recorded_as_no_attempt` to
   `liquers-core/src/assets.rs` `mod tests`; confirm it fails at HEAD (`Persisted`).
   Proof: `cargo test -p liquers-core --lib cancelled_asset_save`.
2. **Outcome type.** Add private `enum SaveOutcome { Written, Skipped }` beside
   `PersistenceStatus`, with the Phase 2 doc comments. Widen `PersistenceStatus::None`'s doc.
3. **`save_to_store`.** Change the return type to `Result<SaveOutcome, Error>`; the two
   cancellation returns and the `!metadata.stored()` return become `Ok(SaveOutcome::Skipped)`;
   the write path returns `written.map(|()| SaveOutcome::Written)` after the existing
   `refresh_listing_version` on success. Depends on 2.
4. **`record_persistence_result`.** Take `Result<SaveOutcome, Error>`; add the
   `Skipped → None` arm (explicit arms, no `_ =>`). Depends on 3. Proof: step-1 test passes;
   `cargo check -p liquers-core --target wasm32-unknown-unknown` covers the wasm branch of
   `persist_with_status_tracking`.
5. **Remaining tests.** Add the other four Phase 3 tests; update the ≈11743 test to assert
   `SaveOutcome::Written`. Proof: `cargo test -p liquers-core --lib --tests`.
6. **Docs and records.** Review `reference/ASSET_LIFECYCLE.md` and
   `reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` for `PersistenceStatus::None` semantics
   (History row + `reviewed:` if edited). Close the issue with a resolution naming the tests.
   Regenerate and check the docs index.
7. **Checks.** `cargo fmt`, `cargo clippy -p liquers-core`, `cargo test -p liquers-lib --lib
   --tests`. Diff review: no new `PersistenceStatus` variant, no change to cancellation check
   order, no `println!`.

## Final Review

Consistent across phases; the change is private except for the corrected status value.
Rollback: revert steps 2-4 together.
