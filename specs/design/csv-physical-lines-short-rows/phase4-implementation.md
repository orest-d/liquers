# Phase 4: Implementation Plan

Precondition: question 1 decided (refuse; or refuse + option).

1. `ParsedRow` + line tracking in `parse_rows`. Proof: T4 (no behaviour change yet).
2. `row_position`, strict `check_row_width`, and message updates in `cell_value`. Apply to
   `read_inferred`. Proof: T1, T2, T3, T5. Agent: sonnet tier; liquers-unittest.
3. Fix tests that asserted short-row leniency or `"CSV row"` text. Proof:
   `cargo test -p liquers-records --all-features --lib --tests`,
   `cargo test -p liquers-lib --lib --tests` (records tests read CSV).
4. Reference notes, issue resolution, index. Diff review.
