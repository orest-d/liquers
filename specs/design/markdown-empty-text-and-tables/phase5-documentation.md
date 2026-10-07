# Phase 5: Documentation - Markdown tables round-trip empty text and state the one-table rule

**Status: executed 2026-10-07**, after implementation (Wave 4 step 27 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update

| Document | Change |
|---|---|
| `reference/RECORD_STREAMS.md` | Markdown notes: the `<!---->` marker (citing CommonMark §6.6), how a hand-written empty cell and the marker read, first table only; format table row. History row |
| module doc of `liquers-records/src/formats/markdown.rs` | escape-table row for empty text; the "but one" exception dropped; the one-table rule |

### Candidates Considered and Discarded

The HTML writer, which needs no change because `<td></td>` already differs from absence.
`RECORD_STREAM_GUIDE.md` has no Markdown notes.

### Issues to Close

`MARKDOWN-TABLE-CANNOT-DISTINGUISH-NULL-FROM-EMPTY-TEXT`.

## Implementation Summary

`liquers-records/src/formats/markdown.rs`:
- `EMPTY_TEXT_MARKER` and `read_cell`, where a trimmed `<!---->` is `None`. Reader rows hold
  `Option<String>`.
- In a `Text` field the marker is `""`; in any other field it is null. A non-nullable non-`Text`
  field refuses it, as it refuses an empty cell.
- The inferring reader treats the marker as an empty string rather than a null, the same way CSV
  treats a quoted `""`.
- The writer emits the marker for `Text("")`, through an explicit match over `FieldValue`.
- `read_markdown`'s doc comment states the first-table rule.

Tests:
- `markdown_empty_text_round_trips` (E1, T1, under both readers; asserts the written text)
- `markdown_empty_cell_is_null` (T2)
- `markdown_empty_text_marker_in_int_column_is_null` (T3)
- `markdown_reads_only_first_table` (T4)

## Documentation Delivered

As planned above.

## Issues Filed

None.

## Important Learning

Because the writer already escaped `<`, the marker cannot collide with any written text. The
marker therefore needed no escape rule of its own.

## Conformance and Remaining Work

Conforms to Phases 1–4. No remaining work.

## Validation

- `cargo test -p liquers-records --all-features --lib --tests`: passed (404 unit tests)
- Full lib loop and build matrix run with the wave
