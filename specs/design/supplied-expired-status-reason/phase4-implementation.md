# Phase 4: Implementation Plan

Preconditions: the decision is accepted, and `immediate-set-state-status-match` is implemented (or
both are done in one PR).

1. Add `ensure_supplied_expiry_reason` (Phase 2) in `assets.rs`. Proof: `cargo check -p liquers-core`.
2. Call it at the four sites. Proof: T1–T4. Agent: sonnet tier; rust-best-practices.
3. Run `cargo test -p liquers-core --lib` and fix any test that compared full metadata (expected
   reason/log additions only).
4. Add the reference sentence (History, `reviewed:`), close the issue, regenerate and check the
   index.
5. Diff review.
