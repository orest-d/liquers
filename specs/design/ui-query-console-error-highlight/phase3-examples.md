# Phase 3: Examples and Tests

## Examples

A failed `ns-pl/head-extra` query with a known action-parameter position yields exactly one
`StyledQueryToken::Highlight` for the matching token while retaining the red message. An error with
`Position::unknown()` yields the same tokens as today and only the message is visually emphasized.

## Tests

1. In `liquers-lib/src/egui/widgets.rs`, test the position-aware conversion before font layout:
   a known parsed-token position produces an underlined layout section and the original wrapper
   remains un-underlined.
2. In `query_console_element.rs`, update with an `AssetSnapshot` containing a positioned error and
   assert that the layouter-facing position is retained. Existing `None`/unknown-error coverage
   continues to define the message-only path.
3. Compile `liquers-lib` with `--no-default-features --features egui --tests` to catch cfg/lifetime
   mistakes.

The existing core styling contract is exercised through the egui conversion test; this change does
not alter `liquers-core`. Use sync unit tests and existing UI fixtures; no screenshot assertion is
required for a token-classification change.
