# Phase 4: Implementation Plan

1. Move the parsers and add `FieldValue::parse_text`. Proof: T1,
   `cargo test -p liquers-records --all-features --lib`.
2. `expand_basic_date`, `expand_basic_timestamp`, and the new `parse_id_value`. Proof: T2–T4.
   Agent: haiku tier; rust-best-practices.
3. T5 (records-gated test in `liquers-lib/tests/`, e.g. `records_end_to_end.rs`). Validate E1/E2
   first. Proof: `cargo test -p liquers-lib --lib --tests`.
4. Build matrix (`records` features).
5. RECORD_STREAMS.md and QUERY_ESCAPING_GUIDE rows, issue resolution quoting the decision, index.
   Diff review.
