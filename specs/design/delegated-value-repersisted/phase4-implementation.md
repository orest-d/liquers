# Phase 4: Implementation plan

1. In `liquers-core/src/assets.rs`, add the private recipe-evaluation outcome and mark only the successful owner hand-off as delegated. Preserve error propagation and normal state ownership. Proof: existing asset unit tests; rollback is removing the wrapper and its gate together.
2. Update `evaluate_and_store` to retain state installation/readiness transitions but skip `persist_with_status_tracking` for delegated outcomes only. Proof: Phase 3 counter assertions.
3. Add the counting-store regression coverage in `liquers-core/tests/manager_parametric.rs` for default and immediate managers. Run keyed delegation tests and `cargo test -p liquers-core --lib --tests`.
4. Update issue/design records and generated index; run docs check, formatting, focused tests, and final diff review for persistence changes outside the delegation branch.
