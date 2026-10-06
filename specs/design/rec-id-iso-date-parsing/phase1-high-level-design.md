# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The text formats already define the canonical spelling (ISO 8601 date,
  RFC 3339 with `Z`), and the parsers exist. Accepting the raw number as a fallback keeps every
  current query working. The query spelling was checked with `liquers-validate`.
- **Open questions:** None. Accepting both is backward compatible, so it does not need a decision.

## Problem

`parse_id_value` (`liquers-lib/src/records/commands.rs`) parses a `Date` id as `i32` (days since
the epoch) and a `Timestamp` id as `i64` (microseconds). Every text format shows these as ISO text,
so `…/ns-rec/rec_id-2026~09~27` fails and the user must compute `20723`. The ISO parsers
`parse_date` / `parse_timestamp` exist in `liquers-records/src/formats/csv.rs` but are
`pub(super)`.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Query spelling (validated)

`-` separates parameters, so a date is written with `~` before each inner dash, and a colon with
`~ncolon~`:

| Value | Query parameter | Checked |
|---|---|---|
| `2026-09-27` | `rec_id-2026~09~27` | `liquers-validate`: parameter decodes to `2026-09-27` |
| `2026-09-27T10:00:00Z` | `rec_id-2026~09~27T10~ncolon~00~ncolon~00Z` | decodes to `2026-09-27T10:00:00Z` |
| `20723` | `rec_id-20723` | unchanged |

## Expected behaviour and acceptance

1. A table whose `Id` field is `Date`: `rec_id-2026~09~27` selects the record dated 2026-09-27.
2. `rec_id-20723` selects the same record (raw days, backward compatible).
3. A `Timestamp` id: RFC 3339 text and raw microseconds both work.
4. Unparseable input: a conversion error naming both accepted spellings.

## Scope

`rec_id`'s id parsing, plus a public parser in `liquers-records`.

## Design Dependencies

- `record-timestamp-utc` — **overlaps**. The timestamp parser assumes UTC instants, which that
  design confirms. No ordering constraint: RFC 3339 with an offset is already converted to an
  instant.

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, `rec_id` row: the spellings table above.
- Guide: `RECORD_STREAM_GUIDE.md`, if it shows `rec_id` with a date.

## Consolidated Findings

- Expose one shared entry point rather than two helpers:
  `FieldValue::parse_text(data_type: FieldType, text: &str) -> Result<FieldValue, Error>` in
  `liquers-records/src/column.rs` (where `FieldValue` lives). Its Date and Timestamp arms call the
  existing parsers, moved from `csv.rs` to a shared module (`formats/text.rs`), which the CSV,
  Markdown and other text readers then import.
- `rec_id` applies `parse_text`. For Date/Timestamp, it falls back to the raw integer on failure.
  The fallback lives in `rec_id`, not in `parse_text`, so file readers stay strict.
