# Phase 4: Implementation Plan

1. Move the parsers and add `FieldValue::parse_text` in `liquers-records`. Proof: T1,
   `cargo test -p liquers-records --all-features --lib`.
2. Rewrite `parse_id_value` (Phase 2). Proof: T2, T3. Agent: haiku tier; rust-best-practices.
3. T4 in `liquers-lib/tests/` (records-gated; e.g. extend `records_end_to_end.rs`). Validate E1
   with `liquers-validate` first. Proof: `cargo test -p liquers-lib --lib --tests`.
4. Build matrix (`records` feature combinations).
5. Reference row, issue resolution, index. Diff review.
