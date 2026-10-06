# Phase 2: Solution and Architecture

## Writer

In `markdown.rs`, where a Text cell is formatted (the caller of `escape_markdown_cell`):
`FieldValue::Text(s) if s.is_empty() → "<!---->"`. Null still writes an empty cell. Add the row
`| empty text | <!----> | GFM has no empty-string literal; an empty comment renders as nothing |`
to the escape table in the module doc.

```rust
const EMPTY_TEXT_MARKER: &str = "<!---->";
```

## Reader

Where cells are converted for a Text field: a trimmed cell equal to `EMPTY_TEXT_MARKER` →
`FieldValue::Text("")`, before `unescape_markdown_cell`. For non-Text fields, `<!---->` → null
(the cell is empty to a reader).

## Documentation

Module doc: drop "for every cell but one…" and state full round-trip. Add "The table read is the
first run of consecutive lines containing `|`; later tables and surrounding text are ignored" to
`read_markdown`'s doc and the reference.

## Known-issue preflight

None.

## Relevant commands

`ns-rec/to_record-md` (or the Markdown format name the registry uses) and writing a view as
`.md`.

## Documentation architecture

RECORD_STREAMS.md Markdown notes, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `formats/markdown.rs` |
| Data | Newly written `.md` tables contain `<!---->` for empty strings. Older readers read it as the literal text `<!---->`, which is acceptable because the format has no version. |
| Existing tests | Round-trip tests that excluded empty text can include it |
| Recovery | Revert |
| Certainty | High once decided |
