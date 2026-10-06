# Phase 4: Implementation Plan

1. Add `LiquersError::construct` (Phase 2). Proof:
   `cargo check -p liquers-web --target wasm32-unknown-unknown`. Agent: haiku tier;
   rust-best-practices.
2. T1, T2 in `objects_OBJECT.rs`, and T3 in `store_js_STORE.rs`. Proof: the Node loop command.
3. T4: the `valid_usage.ts` line, then build and `check-stubs.sh`.
4. README error section (+ the language-integration guide if it covers the bridge). Issue
   resolution, index. Diff review.
