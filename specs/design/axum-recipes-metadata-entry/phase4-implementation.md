# Phase 4: Implementation Plan

1. Add `recipe_metadata` and rewrite both handlers (Phase 2). Proof: `cargo check -p liquers-axum`.
   Agent: haiku tier; rust-best-practices.
2. Add E1–E3 and update existing expectations in `tests/recipes_api_routes.rs`. Proof:
   `cargo test -p liquers-axum --test recipes_api_routes`.
3. `cargo test -p liquers-axum` (all suites).
4. Spec rows (History, `reviewed:`), issue resolution, index. Diff review.
