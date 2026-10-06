# Phase 4: Implementation Plan

Preconditions: questions 1–2 decided (proceed now, subset list).

1. `TypeInfo` field, builder, `can_read_data_format` (TypeInfo + registry). Doc comment fix. Proof:
   T1, T2, `cargo test -p liquers-core --lib type_system`. Agent: sonnet tier; rust-best-practices.
2. Fast-track check and recovery-read error. Proof: T3.
3. `RecordView` declaration. Proof: T4, T5. `cargo test -p liquers-lib --lib --tests`.
4. `cargo check -p liquers-py -p liquers-axum`; the web crate per CLAUDE.md (after `cargo clean`)
   if `TypeInfo` is constructed there.
5. Docs, issue resolution, index. Diff review.
