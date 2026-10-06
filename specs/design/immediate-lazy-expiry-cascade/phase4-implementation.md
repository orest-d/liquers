# Phase 4: Implementation Plan

1. Switch both lazy sites to `expire_with_reason` (Phase 2). Proof: `cargo check -p liquers-core`.
   Agent: haiku tier; rust-best-practices.
2. Lock check: read `cascade_expire_dependents` and its callees for `key_mutation_lock`, and record
   the finding in the PR. Containment: if a re-entrant acquisition exists, move the call after the
   unmapping block, outside the lock.
3. Scenario tests E1–E3; update T1. Proof: `cargo test -p liquers-core --lib --tests`.
4. Web: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`
   (after `cargo clean`).
5. Docs (`DEPENDENCIES_STATUS.md`, History, `reviewed:`), code comments, issue resolution quoting
   the decision, index. Diff review.
