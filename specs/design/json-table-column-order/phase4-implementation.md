# Phase 4: Implementation Plan

Precondition: question 1 accepted (local parse).

1. Inventory: list every function in `liquers-records` that takes `serde_json::Value` for a table,
   and confirm whether any is public. Proof: a note in the PR.
2. Add `OrderedMap<V>` with a `Deserialize` impl and a unit test. Proof:
   `cargo test -p liquers-records --all-features --lib ordered`.
3. Switch the schema-less readers to it (first-appearance ordering). Proof: T1–T5.
   Agent: sonnet tier; rust-best-practices; knowledge: `formats/shapes.rs`, `ordered_row_keys`.
4. T6 checks. `cargo test -p liquers-lib --lib --tests` (records conversions).
5. Reference sentence, issue resolution, index. Diff review.
