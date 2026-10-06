# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): rows shorter than the header "may be padded with nulls or
  empty strings. They should not be silently ignored. A warning should be written to log that a row
  has been padded … in an aggregate way for the whole CSV". The design adds a read report that
  carries the aggregate warning to the asset log.
- **Open questions:** None

## Problem

`liquers-records/src/formats/csv.rs`:

- Errors say "CSV row N", where N counts records. A quoted cell spanning lines makes N disagree with
  the editor's line.
- A row with fewer cells than the header is accepted, and its missing cells silently become null.
  A truncated line in a non-nullable column then fails later with an error about nulls rather
  than about the line.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Expected behaviour and acceptance

1. Every per-row **error** names the physical line where the record starts, and the record number
   when they differ: "CSV line 7 (record 5)".
2. A **short row is padded**: a missing cell is null when the field is nullable, and `""` when the
   field is a non-nullable `Text`. A missing cell in any other non-nullable field is an error
   naming the line (there is nothing honest to pad with).
3. Padding is **reported once per read**, aggregated, with the decided wording:
   "There has been A rows with number of cells between B and C, which is less than number of
   columns in the header (D)." A = number of padded rows, B/C = min/max cell count among them,
   D = table width.
4. The warning reaches the asset log when the read happens inside a command (`ns-rec/to_record`
   and every command going through `records::convert::to_record`), via `Context::warning`.
5. A row wider than the header is still refused (unchanged), with the line-based message.
6. A trailing newline at the end of the file is not a row.

## Scope

The CSV/TSV reader, the read API's report, and the one conversion entry point that has a context.

**Out of scope, documented:** a CSV read during deserialization of a stored `RecordView`
(`ExtValue` type registry, `liquers-lib/src/value/mod.rs`) has no log to write to. Its report goes
to stderr (`eprintln!`). Those files are written by Liquers and are never short, so this path
should not occur in practice.

## Design Dependencies

- `markdown-empty-text-and-tables` — **overlaps** (reader consistency).
- `rec-id-iso-date-parsing` — **overlaps**. It moves `parse_date`/`parse_timestamp` out of
  `csv.rs`. The two touch the same file, so order them (rec-id first).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, CSV notes: padding rule, the aggregate warning,
  and line numbering.

## Consolidated Findings

- `parse_rows` is hand-written and is the only place that knows where a record starts, so it
  returns the start line with each row.
- `read_table` has no channel for warnings. Adding `read_table_with_report` (returning a
  `ReadReport`) keeps `read_table`'s signature for its many callers. `read_table` becomes a wrapper
  that sends report lines to stderr.
- The doc comment of `check_row_width` documents the old silent leniency, and changes with this.
