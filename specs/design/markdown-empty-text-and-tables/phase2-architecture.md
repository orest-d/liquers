# Phase 2: Solution and Architecture

```rust
/// An empty `Text` cell. GFM has no empty-string literal; an empty HTML comment is valid
/// CommonMark (0.30, 0.31.2 §6.6) and renders as nothing.
const EMPTY_TEXT_MARKER: &str = "<!---->";
```

- **Writer:** for a `Text` field, `Some("")` → `EMPTY_TEXT_MARKER`. Null still writes an empty
  cell. Add the escape-table row `| empty text | <!----> | an empty HTML comment renders as
  nothing |` to the module doc.
- **Reader:** before `unescape_markdown_cell`, a trimmed cell equal to `EMPTY_TEXT_MARKER` is
  `Text("")` in a Text field and null in any other field.
- **Docs:** `read_markdown`'s doc comment: "the table read is the first run of consecutive lines
  containing `|`; later tables and surrounding text are ignored".

## Known-issue preflight

None.

## Relevant commands

Reading/writing views as `.md` (`ns-rec/to_record-md` or the registry's Markdown format name).

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-records/src/formats/markdown.rs` |
| Data | New `.md` output contains `<!---->` for empty strings. Older readers see the literal text. Acceptable, because the format has no version. |
| Recovery | Revert |
| Certainty | High |
