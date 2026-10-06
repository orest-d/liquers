# Phase 4: Implementation Plan

Land after `rec-id-iso-date-parsing` if both are scheduled (same file).

1. `ParsedRow` and line tracking in `parse_rows`. Proof: existing CSV tests pass.
2. `ReadReport`, `read_table_with_report`, and `read_table` as a wrapper (warnings to stderr).
   Proof: `cargo test -p liquers-records --all-features --lib`.
3. `ShortRows`, padding in `cell_value`, the width check in both readers. Proof: T1–T6.
   Agent: sonnet tier; liquers-unittest.
4. `convert::to_record` logs report warnings. Proof: T7, `cargo test -p liquers-lib --lib --tests`.
5. Fix tests asserting `"CSV row"` text. Run the build matrix (`records` features).
6. Reference notes, issue resolution quoting the decision, index. Diff review.
