# Phase 4: Implementation Plan

1. `asset_info_to_js` in `store/mod.rs`, and `getAssetInfo` in `wrapper.rs`. Proof:
   `cargo check -p liquers-web --target wasm32-unknown-unknown`. Agent: haiku tier;
   rust-best-practices.
2. T1–T3 in the store wrapper tests (find the existing `getMetadata` wrapper test file under
   `liquers-web/tests/`). Proof: the Node loop command.
3. `valid_usage.ts` line, then build + `check-stubs.sh`. Proof: T4.
4. README, issue resolution, index. Diff review.
