# Phase 3: Examples & Use-cases — UUID, time-of-day and duration record field types

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Arrow file with the three types | `-R/ops/jobs.arrow/-/ns-rec/head-5` | AC-1 |
| 2 | Example | CSV under a declared schema | `jobs.csv` with a manifest schema | AC-2, AC-10 |
| 3 | Example | Filter and sort | `ns-rec/gt-took-PT60S/ns-rec/sort-starts` | AC-8 |
| 4 | Test | `temporal.rs::tests::*` | text forms and parsers | AC-2, AC-3, AC-4 |
| 5 | Test | `formats/ipc.rs::tests::uuid_time_duration_round_trip` | IPC | AC-1 |
| 6 | Test | `formats/ipc.rs::tests::time_and_duration_units_scale_exactly` | units | AC-4 |
| 7 | Test | `formats/ipc.rs::tests::fixed_size_binary_without_uuid_extension_refused` | refusal | AC-5 |
| 8 | Test | `formats/ipc.rs::tests::interval_refused_with_text_hint` | refusal | AC-11 |
| 9 | Test | `formats/csv.rs::tests::declared_uuid_time_duration_columns` | text formats | AC-2, AC-3 |
| 10 | Test | `column.rs::tests::uuid_time_duration_compare_and_sort` | filters, sort | AC-8 |
| 11 | Test | `schema.rs::tests::new_field_types_serialize_in_yaml` | schema | AC-10 |
| 12 | Test | `liquers-lib` `records/polars.rs` tests `polars_bridge_time_duration_uuid` | bridge | AC-6 |
| 13 | Test | `liquers-lib` `records` Parquet test `parquet_uuid_time_duration_columns` | Parquet | AC-7 |
| 14 | Test | `liquers-web` records suite `uuid_time_duration_columns_cross` | browser | AC-9 |
| 15 | Test | `liquers-lib/tests` `ns_rec_filters_on_new_types` | commands | AC-8 |

## Example 1: an Arrow file with the three types

`ops/jobs.arrow` (written by the test with `liquers-records`' own writer, then hand-checked once
against `pyarrow` for the extension name):

| id (`Uuid`) | starts (`Time`) | took (`Duration`) |
|---|---|---|
| `0190c2a4-7d3e-7b1a-9c3f-2a4b5c6d7e8f` | `10:15:30` | `PT90.5S` |
| `0190c2a4-7d3e-7b1a-9c3f-2a4b5c6d7e90` | `08:00:00.25` | `PT60S` |
| null | `23:59:59.999999` | `-PT1S` |

`-R/ops/jobs.arrow/-/ns-rec/head-5` (validated) gives these three rows; `…/ns-rec/head-5/jobs.csv`
writes them as the text above; JSON writes `"id":"0190c2a4-…","starts":"10:15:30","took":"PT90.5S"`.

## Example 2: a CSV under a declared schema

`ops/jobs.csv` holds the same rows as text; a manifest declares
`schema: [{name: id, data_type: Uuid}, {name: starts, data_type: Time}, {name: took, data_type: Duration}]`.
Reading gives the typed view of Example 1. Without the schema the three columns infer as `Text`
(question 5).

## Example 3: filter and sort

`-R/ops/jobs.arrow/-/ns-rec/gt-took-PT60S/ns-rec/sort-starts` (validated) keeps only the first
row (`PT90.5S > PT60S`; `PT60S` is not greater; the null row drops). `ns-rec/sort-starts` alone
orders `08:00:00.25`, `10:15:30`, `23:59:59.999999`.
`-R/ops/jobs.arrow/-/ns-rec/eq-id-0190c2a47d3e7b1a9c3f2a4b5c6d7e8f` (validated) selects the first
row; the hyphenated form must be written with `~_` escapes in a query.

## Edge and Error Cases

| Case | Expected | Scenario |
|---|---|---|
| `not-a-uuid`, 31 hex digits | conversion error naming the field | AC-3 |
| `24:00:00`, `25:00:00`, `10:61:00` | conversion error | AC-3 |
| `P1M`, `P1Y` | conversion error ("calendar units are not a duration") | AC-3 |
| `P1DT2H` | `93600000000` µs, written back as `PT93600S` | AC-2 |
| `Time64(ns)` value `1` | conversion error naming the field | AC-4 |
| `Time32(ms)` `1500` | `00:00:01.5` | AC-4 |
| `FixedSizeBinary(16)` without extension; `FixedSizeBinary(8)` | `not_supported`, as today | AC-5 |
| `Interval(MONTH_DAY_NANO)` | `not_supported`, message suggests a `Text` cast | AC-11 |
| concat of `Time` with `Duration` columns | schema mismatch error, as for any type mismatch | AC-1 |
| `Uuid` through polars and back | `Text` with canonical values | AC-6 |
| `Duration` through Parquet and back | `Int`, equal microseconds | AC-7 |

## Test Plan

- `liquers-records` unit tests 4-11 (each module's `mod tests`), run with
  `cargo test -p liquers-records --all-features --lib --tests`, and the IPC ones also without
  features skipped by their gates (`cargo test -p liquers-records --lib --tests`).
- `liquers-lib` tests 12, 13, 15 (features `records`, `polars`, `records-parquet`):
  `cargo test -p liquers-lib --lib --tests`.
- `liquers-web` test 14:
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records`
  and `./liquers-web/scripts/check-stubs.sh --build --records`.
- `field_value_stays_reasonably_small` must still pass unchanged.
