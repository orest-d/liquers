# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — the written form of an empty `Text` cell.** It is
  part of the Markdown table format, which persisted files and readers depend on.
- **Explanation:** The writer already escapes `<` as `\<`, so any raw `<` in a written cell can
  only come from the writer itself. That makes an HTML comment an unambiguous marker that GFM
  renders as nothing. A working design is specified around it.
- **Open questions:**
  1. **Proposed resolution — empty text is written `<!---->`:** an empty HTML comment. It renders
     as an empty cell in GFM, so the table looks the same to a reader. It cannot collide with
     escaped text, and the reader maps exactly that cell content to `Some("")`. Alternatives:
     `""` (collides with the literal two-quote text unless quotes are escaped too); `&#8203;`
     (a zero-width space is a real character, so it would round-trip as itself).
  2. **Proposed resolution — several tables:** keep reading the first table only and state it in
     the reference and the module doc (it is already the module doc's behaviour: "the first run of
     consecutive lines containing a `|`"). Refusing a document with a second table would break
     reading a table out of a larger Markdown document, which is a deliberate use.

## Problem

`liquers-records/src/formats/markdown.rs` writes null and empty `Text` both as an empty cell, so
`read(write(x)) != x` for empty text. The module doc lists this as the one exception. The reader
stops after the first table, and nothing outside the code says so.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. `read(write(x)) == x` for a Text column containing `None`, `Some("")`, `Some(" ")` and
   `Some("a")`.
2. A hand-written empty cell still reads as null (unchanged).
3. `<!---->` in a hand-written file reads as empty text, and `<!-- note -->` (non-empty comment)
   is read as its literal text, unchanged from today (the reader decodes only the escapes it
   documents). Phase 4 verifies how `unescape_markdown_cell` treats `<`.
4. The reference states: "only the first table of a document is read; text and later tables are
   ignored".

## Scope

Text columns. Other types have no empty-vs-null ambiguity (an empty cell is null).

## Design Dependencies

- `csv-physical-lines-short-rows` — **overlaps** (format consistency).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, Markdown format notes (empty text spelling, first
  table only).
- Module doc in `markdown.rs` (escape table gains a row; the "but one" exception sentence goes).

## Consolidated Findings

- The writer escapes `<` to `\<`, so a cell that is exactly `<!---->` can only be the empty-text
  marker or hand-written. Hand-written means "empty", so either way the intent is empty text.
- The HTML writer (`formats/html.rs`) has no such ambiguity (`<td></td>` vs absent) and is out of
  scope.
