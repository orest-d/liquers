# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): follow the recommendation after checking CommonMark. The
  recommended marker, an empty HTML comment `<!---->`, is a valid comment in CommonMark (0.30 and
  0.31.2) and renders as nothing, so a table looks the same to a reader. The writer escapes `<`, so
  the marker cannot collide with escaped text.
- **Open questions:** None

## CommonMark check

- **CommonMark 0.31.2, §6.6 "Raw HTML":** an HTML comment is `<!-->`, `<!--->`, or `<!--` followed
  by text not containing `-->`, followed by `-->`. `<!---->` is `<!--`, empty text, `-->`, so it is
  valid.
- **CommonMark 0.30** (stricter): comment text must not start with `>` or `->`, not end with `-`,
  and not contain `--`. Empty text satisfies all of them, so it is valid there too.
- **GFM tables:** cell content is parsed as inlines, and raw inline HTML (including comments) is
  allowed in cells. GFM's tag filter applies only to a fixed list of tags (`title`, `textarea`,
  `style`, `xmp`, `iframe`, `noembed`, `noframes`, `script`, `plaintext`), not to comments.
- **Rendered result:** an empty cell, the same as a null cell to a human reader. The two
  differ only in the source, which is what a reader needs to tell them apart.

## Problem

`liquers-records/src/formats/markdown.rs` writes null and empty `Text` both as an empty cell, so
`read(write(x)) != x` for empty text (the module doc lists this as the one exception). The reader
stops after the first table, which nothing outside the code states.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. `read(write(x)) == x` for a Text column containing `None`, `Some("")`, `Some(" ")`, `Some("a")`.
2. A hand-written empty cell still reads as null.
3. A cell that is exactly `<!---->` (after trimming) reads as `""` in a Text column and as null in
   other columns.
4. Only the first table of a document is read, and text and later tables are ignored. Stated in
   the module doc and the reference. Refusing multi-table documents would break reading a table out
   of a larger document.

## Design Dependencies

- `csv-physical-lines-short-rows` — **overlaps** (reader consistency).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, Markdown notes (empty text marker, first table only).
- Module doc in `markdown.rs` (escape table row; drop the "but one" exception).

## Consolidated Findings

- The writer escapes `<` to `\<`, so a raw `<!---->` cell can only come from the writer or a
  person meaning "empty". Either way the intent is empty text.
- The HTML writer is unaffected (`<td></td>` differs from absence).
