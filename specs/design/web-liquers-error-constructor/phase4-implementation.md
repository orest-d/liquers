# Phase 4: Implementation Plan

1. Add `LiquersError::construct` (Phase 2). Proof:
   `cargo check -p liquers-web --target wasm32-unknown-unknown`. Agent: haiku tier;
   rust-best-practices.
2. T1, T2, T2b, T2c in `objects_OBJECT.rs`, and T3 in `store_js_STORE.rs`. Proof: the Node loop command.
3. T4: the `valid_usage.ts` line, then build and `check-stubs.sh`.
4. README error section (+ the language-integration guide if it covers the bridge). Issue
   resolution, index. Diff review.

## Outcome (2026-10-11)

Steps 1–4 done; T1, T2, T2b, T2c (`objects_OBJECT.rs`), T3 (`store_js_STORE.rs`, labelled
`ERROR06` after the language-integration guide's test) and T4 pass, and `check-stubs.sh` passes.
Two differences from Phase 2:

- **`Error::from_error`, not `Error::new`.** `from_error` takes a runtime `ErrorType` and any
  `Display`, so the typed-constructor rule needs no exception.
- **The first parameter is named `errorType` in JavaScript** (`#[wasm_bindgen(js_name)]` on the
  parameter); without it the generated declaration read `error_type`.

The language-integration guide needs no change: it already asks for `ERROR06`. The README's store
section shows how a page raises a typed error.
