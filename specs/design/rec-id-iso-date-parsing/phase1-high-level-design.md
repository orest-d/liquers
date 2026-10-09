# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — changes a command's argument parsing across `liquers-records`
  and `liquers-lib` (rule 6)
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): the recommended date format in Liquers command arguments is
  `YYYYMMDD`, or `YYYY-MM-DD` after expansion, which is written `YYYY~MM~DD` with tilde escaping.
  Both spellings were checked with `liquers-validate`.
- **Open questions:** None

## Problem

`parse_id_value` (`liquers-lib/src/records/commands.rs`) parses a `Date` id as `i32` (days since
the epoch) and a `Timestamp` id as `i64` (microseconds). Tables show ISO text, so a user has to
compute `20723` by hand. The ISO parsers `parse_date` / `parse_timestamp` exist in
`liquers-records/src/formats/csv.rs` but are `pub(super)`.

Filed during `record-streams` (complete, frozen). The issue's `design:` field pointed at that folder; it now points here, because a frozen design cannot own new work.

## Accepted spellings (validated)

| Type | In the query | Parameter value received | Meaning |
|---|---|---|---|
| Date | `rec_id-20260927` | `20260927` | ISO 8601 basic date |
| Date | `rec_id-2026~09~27` | `2026-09-27` | ISO 8601 extended date (recommended when readability matters) |
| Timestamp | `rec_id-20260927T100000Z` | `20260927T100000Z` | ISO 8601 basic date-time, UTC |
| Timestamp | `rec_id-2026~09~27T10~ncolon~00~ncolon~00Z` | `2026-09-27T10:00:00Z` | RFC 3339 |

`liquers-validate` confirms each query parses to one parameter with the value in the third column.

**Raw epoch numbers are no longer accepted.** An 8-digit date would be ambiguous between
`YYYYMMDD` and a day count, and the decision names the two formats. Queries using raw numbers
(unlikely: the issue describes them as something users had to compute) get an error naming the
accepted spellings.

## Expected behaviour and acceptance

1. A table whose `Id` field is `Date`: both date spellings select the 2026-09-27 record.
2. A `Timestamp` id: both timestamp spellings select the record at 2026-09-27T10:00:00Z.
3. Anything else, including `20723`, is a conversion error whose message lists the spellings,
   including the `~` form for the query.

## Scope

`rec_id`'s id parsing, plus a shared public parser in `liquers-records`. The text-format readers
keep accepting extended ISO only, the format the writers produce.

## Design Dependencies

- `record-timestamp-utc` — **overlaps** (timestamps are UTC instants).
- `csv-physical-lines-short-rows` — **overlaps** (same file `csv.rs`; land this first).

## Documentation assessment

- Reference: `specs/reference/RECORD_STREAMS.md`, `rec_id` row, with the spellings table.
- Guide: `specs/guides/QUERY_ESCAPING_GUIDE.md`, one example row: `2026-09-27` → `2026~09~27`
  (the recommended date spelling in arguments).

## Consolidated Findings

- Move `parse_date`/`parse_timestamp` to a shared module and expose
  `FieldValue::parse_text(FieldType, &str)` for the extended forms. `rec_id` adds the basic forms
  (`YYYYMMDD`, `YYYYMMDDTHHMMSSZ`) by normalizing them to the extended form before parsing:
  insert the separators, then call the same parser. One parser, two spellings.
