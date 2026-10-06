# Phase 4: Implementation Plan

Precondition: questions 1–2 accepted.

1. Writer marker + module-doc table row. Proof: E1 (`markdown_empty_text_round_trips` write half).
2. Reader marker handling (check how `unescape_markdown_cell` handles `<` first). Proof: T1–T3.
   Agent: haiku tier; liquers-unittest.
3. T4 + the `read_markdown` doc sentence.
4. `cargo test -p liquers-records --all-features --lib --tests`; `cargo test -p liquers-lib --lib --tests`.
5. Reference notes, issue resolution, index. Diff review.
