# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — refuse short rows without a lenient option.**
  Refusing changes what files read successfully (observable data behaviour). Today a row with
  fewer cells than the header reads its missing cells as null.
- **Explanation:** Line tracking is an implementation detail. The short-row policy is a
  behaviour choice, specified here around the recommendation.
- **Open questions:**
  1. **Proposed resolution — refuse, no option for now.** A short row is refused like a wide
     one, naming the physical line. A wide row is already refused, so the reader becomes
     symmetric. A `ReadOptions` flag for leniency can be added when a real file needs it (YAGNI).
     Alternative: add `ReadOptions::allow_short_rows: bool` (default `false`) now.

## Problem

`liquers-records/src/formats/csv.rs`:

- Errors say "CSV row N", where N counts records (`row_index + line_base`). A quoted cell spanning
  lines makes N disagree with the editor's line.
- `check_row_width` refuses `row.len() > width` but accepts shorter rows. Missing cells become null
  (in `cell_value` via `row.get(col) == None`), and a truncated line in a non-nullable column fails
  later with an error about nulls rather than about the line.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. Every per-row error names the physical line where the record starts. When it differs from the
   record number, it also names the record: "CSV line 7 (record 5)".
2. A row with fewer fields than the table width is an error naming the line and both counts
   ("has 2 fields, but the table has 3 columns").
3. Both the schema-aware reader (`read_declared`) and the inferring reader (`read_inferred`) apply
   2.
4. A trailing empty line at end of file is not a short row (it is not a record). Phase 4 verifies
   `parse_rows`' handling.

## Scope

The CSV reader only. TSV shares it via the separator, so it is covered too.

## Design Dependencies

- `markdown-empty-text-and-tables` — **overlaps** (Markdown already refuses rows of the wrong
  width and names lines; the two readers become consistent).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, CSV format notes: strict width and line
  numbering.

## Consolidated Findings

- `parse_rows` is a hand-written parser. It is the only place that knows where a record starts,
  so it must return the start line with each row. Make the parser yield `ParsedRow { line: usize,
  fields: Vec<RawField> }`.
- The doc comment of `check_row_width` explicitly documents the short-row leniency. That comment
  changes with the behaviour.
- Existing tests that rely on short rows reading as null must be found (`rg "short" liquers-records`)
  and updated deliberately. They are evidence of who relied on the behaviour.
