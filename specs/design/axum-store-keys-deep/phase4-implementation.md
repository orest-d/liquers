# Phase 4: Implementation Plan

"Deep" chosen by the maintainer on 2026-10-08. All steps done on branch `claude/fix-axum-store-keys-deep`.

1. Change `keys_handler` and its doc (Phase 2). Proof: `cargo check -p liquers-axum`.
2. Add T1–T3. Proof: `cargo test -p liquers-axum --test store_api_routes`.
3. Spec row, issue resolution, index. Diff review.
