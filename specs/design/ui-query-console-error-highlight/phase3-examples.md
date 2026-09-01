# Phase 3: Examples and Tests

## Examples

A failed `ns-pl/head-extra` query with a known action-parameter position yields exactly one
`StyledQueryToken::Highlight` for the matching token while retaining the red message. An error with
`Position::unknown()` yields the same tokens as today and only the message is visually emphasized.

## Tests

1. In `liquers-core/src/query.rs`, assert known matching positions produce `Highlight`, nonmatching
   and unknown positions do not, and concatenated token text still equals the original query.
2. In `liquers-lib/src/egui/widgets.rs`, expose or privately test the position-aware conversion
   before font layout; retain the old wrapper behaviour.
3. In `query_console_element.rs`, update with an `AssetSnapshot` containing a positioned error and
   assert the position selected for the layouter; repeat with no error and unknown position.
4. Compile `liquers-lib` with `--no-default-features --features egui --tests` to catch cfg/lifetime
   mistakes.

Use sync unit tests for token conversion and the existing UI test fixtures; no screenshot assertion
is required for a token-classification change.
