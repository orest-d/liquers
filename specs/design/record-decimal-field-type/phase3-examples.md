# Phase 3: Examples & Use-cases — record Decimal field type

Queries were checked with `./target/debug/liquers-validate` on 2026-10-26, and all three plan as
expected. `-R/finance/ledger.arrow/-/ns-rec/head-5`;
`-R/finance/ledger.arrow/-/ns-rec/gt-amount-10/ns-rec/sort-amount` plans as `rec/gt(amount, 10)`
followed by `rec/sort(amount)`; `-R/finance/ledger.arrow/-/ns-rec/head-5/ledger.parquet` ends in
a `Filename` step.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Reading a decimal IPC ledger | the problem example | AC-1, AC-3 |
| 2 | Example | Filtering money exactly | literal at the column scale | AC-6 |
| 3 | Test | `liquers_records::decimal::tests::parse_and_format_are_exact` | `12.3`→`12.30`, `-0.05`, scale 0, 38 digits | AC-3 |
| 4 | Test | `liquers_records::decimal::tests::no_rounding_and_no_overflow` | `12.345` at scale 2, 11 digits at precision 10 | AC-4 |
| 5 | Test | `liquers_records::decimal::tests::order_across_scales` | `10.0` vs `10.00`, negatives | AC-6 |
| 6 | Test | `liquers_records::column::tests::field_value_stays_reasonably_small` (unchanged) | ≤ 32 bytes with the new variant | AC-1 |
| 7 | Test | `liquers_records::schema::tests::decimal_field_type_serde_json_and_yaml` | JSON, YAML map form, YAML tag form | AC-8 |
| 8 | Test | `liquers_records::formats::ipc::tests::decimal_round_trip_with_nulls` | write → read, Arrow type tag 7, bitWidth 128 | AC-1 |
| 9 | Test | `liquers_records::formats::ipc::tests::decimal256_and_negative_scale_refused` | hand-built schema messages | AC-9 |
| 10 | Test | `liquers_records::formats::csv::tests::decimal_with_declared_schema` | CSV, TSV, Markdown, HTML text forms | AC-3, AC-4 |
| 11 | Test | `liquers_records::formats::ndjson::tests::decimal_is_a_json_string` | JSON and NDJSON, string and number read | AC-3 |
| 12 | Test | `liquers_records::formats::infer::tests::decimals_are_never_inferred` | `12.30` column infers `Float` | AC-3 |
| 13 | Test | `liquers_records::formats::parquet::tests::decimal_column_metadata` | FLBA(16), BE bytes, logical type | AC-2 |
| 14 | Test | `liquers_records::column::tests::decimal_compare_sort_concat` | filter mantissa, concat mismatch error | AC-6 |
| 15 | Test | `liquers-lib/tests/records_polars.rs` `decimal_bridge_round_trip` (`polars`) | both directions, dtype | AC-5 |
| 16 | Test | `liquers-lib/tests/records_polars.rs` `parquet_decimal_reads_back` (`polars`, `records-parquet`) | our Parquet read by polars | AC-2 |
| 17 | Test | `liquers-lib/tests/records_polars.rs` `polars_written_ipc_decimal_reads` (`polars`, `records-ipc`) | interop: polars `IpcWriter` → our reader | AC-1 |
| 18 | Test | `liquers-lib/tests/records_commands.rs` `decimal_filter_and_sort_through_queries` | the query of Example 2 | AC-6 |
| 19 | Test | `liquers-web` Node suite `records_decimal_column` (`debug-handles,records`) | descriptor, copy, single cell | AC-7 |
| 20 | Check | `./liquers-web/scripts/check-stubs.sh --build --records` | `.d.ts` kind union | AC-7 |

## Example 1: a decimal ledger

`finance/ledger.arrow` is written in the test by `write_table` from a batch with `id: Int`,
`amount: Decimal(10,2)` = `[12.30, null, 10.00, 10.01, -0.05]`.

```
-R/finance/ledger.arrow/-/ns-rec/head-5
```

The result has 5 rows. `ns-rec/schema` shows `amount: {Decimal: {precision: 10, scale: 2}}`.
Saved as `ledger.csv`, the `amount` column reads `12.30,,10.00,10.01,-0.05`. As `ledger.json` its
cells are `"12.30"`, `null` and so on. At HEAD the query fails in `decode_arrow_field` with tag 7.

## Example 2: filtering money exactly

```
-R/finance/ledger.arrow/-/ns-rec/gt-amount-10/ns-rec/sort-amount
```

The literal `10` is parsed at scale 2 as `10.00`. The result is two rows, `10.01` then
`12.30`. `10.00`, `-0.05` and the null are excluded.

## Edge and Error Cases

- `ns-rec/gt-amount-10.001` against scale 2 is an error ("not exact at scale 2"), not a rounded
  comparison.
- A CSV cell of `1e2` under a decimal schema is refused, because exponents are not decimal text.
- `Column::concat` of `Decimal(10,2)` and `Decimal(12,2)` is refused. Widening is left to a later
  change.
- A polars `Decimal` with no precision maps to precision 38.
- A decimal schema field applied to an NDJSON number `0.1` parses the number's text, giving exactly
  `0.10` at scale 2, not the nearest double.

## Test Plan

```bash
cargo test -p liquers-records --all-features --lib --tests
cargo test -p liquers-records --lib --tests
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-lib --no-default-features --features records --lib --tests
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles,records
./liquers-web/scripts/check-stubs.sh --build --records
bash scripts/check-build-matrix.sh
```
