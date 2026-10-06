# Phase 4: Implementation Plan

Implement after (or with) `immediate-set-state-status-match`.

1. Add `log_supplied_expiry` (Phase 2). Proof: `cargo check -p liquers-core`.
2. Call it at the four sites. Proof: T1–T4. Agent: sonnet tier; rust-best-practices.
3. `cargo test -p liquers-core --lib --tests`, and fix full-metadata comparisons (expected additions only).
4. Reference sentence, issue resolution quoting the decision and the interpretation of the
   structured field, index. Diff review.
