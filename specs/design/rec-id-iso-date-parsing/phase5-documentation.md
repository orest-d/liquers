# Phase 5: Documentation - ns-rec/rec_id accepts YYYYMMDD and YYYY-MM-DD ids

**Status: executed 2026-10-07**, after implementation (Wave 4 step 24 of
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
| `reference/RECORD_STREAMS.md` | `rec_id` row: the four accepted spellings and the `~` form for a query; raw counts refused. One validated example query. History row (`reviewed:` already 2026-10-07) |
| `guides/QUERY_ESCAPING_GUIDE.md` | Example row `2026-09-27` → `2026~09~27`. History row, `reviewed: 2026-10-07` |

### Candidates Considered and Discarded

`QUERY_LANGUAGE.md` (no date semantics); `specs/command_registry.yaml` (signature unchanged).

### Issues to Close

`REC-ID-PARSES-DATE-AND-TIMESTAMP-IDS-AS-RAW-NUMBERS`.

## Implementation Summary

- `liquers-records/src/column.rs`: `FieldValue::parse_text(FieldType, &str)`, an explicit match
  over every `FieldType`; it calls the CSV cell parser for scalar types and refuses `Binary` and
  `Vector`.
- `liquers-records/src/formats/csv.rs`: `parse_scalar`, `parse_date` and `parse_timestamp` are now
  `pub(crate)`. **Deviation from Phase 2:** they stay in `csv.rs` rather than moving to
  `formats/mod.rs`. `ndjson.rs` already reaches them as `csv::parse_date`, and moving them would only
  churn the file that `csv-physical-lines-short-rows` changes next. The public surface is
  `FieldValue::parse_text`, as planned.
- `liquers-lib/src/records/commands.rs`: `expand_basic_date`, `expand_basic_timestamp` and the new
  `parse_id_value`. Every non-ISO `Date`/`Timestamp` id is a conversion error that names the
  accepted spellings, including the `~` form.

Tests:
- `field_value_parses_iso_date` (records)
- `rec_id_accepts_basic_and_extended_dates`
- `rec_id_accepts_basic_and_extended_timestamps`, which also covers a fractional basic timestamp
- `rec_id_rejects_epoch_day_numbers`
- `rec_id_selects_by_date_query`, in `liquers-lib/tests/records_end_to_end.rs`. It uses a fixture
  command with a `Date` `Id`, because a CSV read without a schema has no `Id`.

## Documentation Delivered

As planned above.

## Issues Filed

None.

## Important Learning

A `-` inside an action parameter separates parameters, so `rec_id-2026-09-27` silently means four
parameters, and `liquers-validate --no-registry` still reports it as `Ok`. That is why the basic
spelling `20260927` is the recommended one.

## Conformance and Remaining Work

Conforms to Phases 1–4, apart from the parser location noted above. No remaining work.

## Validation

- `cargo test -p liquers-records --all-features --lib`: 389 passed
- `cargo test -p liquers-lib --lib records::commands` and `--test records_end_to_end`: passed
- E1, E2 and the RFC 3339 spelling were validated with `liquers-validate`
- Build matrix and full lib loop are run with the wave
