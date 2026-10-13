# Phase 3: Examples & Use-cases — Record and polars command interoperability

All queries below were checked with `liquers-validate`, with the new commands declared through
`--command`. A `-` inside a value is written `~_` (`2026~_10~_02`).

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Narrow, order and hand over | filters + sort + tail in `ns-rec`, the same in `ns-pl` | AC-1, AC-2, AC-7, AC-9, AC-10 |
| 2 | Example | A stream projected before polars | lazy `select_columns` over a source, `materialize`, `ns-pl/sum` | AC-5, AC-18, AC-10 |
| 3 | Tests | `liquers-lib/tests/records_table_ops.rs` | every `ns-rec` command through `evaluate` | AC-1 – AC-15, AC-17, AC-18 |
| 4 | Tests | unit tests in liquers-records and liquers-lib | kernels, wrappers, serde leniency | AC-2, AC-6, AC-7, AC-14, AC-18 |

## Fixture

`orders.csv` from [`schema-example.md`](./schema-example.md) (three rows: A-1 Alice Smith 120.5
`["gift"]`, A-2 Bob Jones 35.0 `[]`, A-3 *null* 99.9 `["bulk","b2b"]`) is stored at
`data/orders.csv` in an `AsyncMemoryStore`, as `search_commands.rs` does.

A fixture command `orders_source` returns an `InMemorySource` of three views, one per row, so that
each source test sees three chunks. It also counts how many chunks were streamed, using the
`OnceLock<Mutex<…>>` pattern from `search_commands.rs`. All tests run through
`envref.evaluate(query)` (`POLARS-COMMAND-TESTS-BYPASS-COMMANDS`).

## Example 1: narrow, order and hand over

```
-R/data/orders.csv/-/ns-rec/to_record-csv/contains-customer-SMITH/sort-amount-f
-R/data/orders.csv/-/ns-rec/to_record-csv/sort-amount-f-t/tail-2
-R/data/orders.csv/-/ns-rec/to_record-csv/ns-pl/sort-amount-f
```

The first query matches A-1 only, because `contains` is case-insensitive by default; with `-t` it
matches none. In the second, `sort-amount-f-t` (descending, nulls last) gives A-1, A-3, A-2, and
`tail-2` keeps A-3, A-2. The third runs `pl/sort` on the record view: same row order, now a
DataFrame without roles.

## Example 2: a stream projected before polars

```
-R/data/orders.manifest.yaml/-/ns-rec/select_columns-amount/materialize/ns-pl/sum
```

`select_columns` over the source returns a `RecordSource`. The materialized batch has two columns,
`amount` and the Id `order_id`, and the polars sum is 255.4. In the test, `orders_source` stands in
for the manifest.

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `eq-customer-a*` | literal match, not a glob | `eq_value_with_wildcards_is_literal` |
| Unknown column, on a view and on a source without a declared schema | error naming it and listing the available ones; on a source, when it is first streamed | `unknown_column_is_refused_naming_available` |
| `is_in` with no values, `drop_columns` of the Id | refused with the command's name | `is_in_without_values_is_refused`, `drop_columns_refuses_the_id_field` |
| `sort` on a Vector column | refused naming the type | `sort_indices_refuses_binary_and_vector` |
| A source into `ns-pl` | refused naming `ns-rec/materialize` | `pl_refuses_a_source_naming_materialize` |
| A Vector column into `ns-pl` | the bridge error stands | `pl_refuses_a_source_naming_materialize` (second assertion) |
| A schema with `FullText` linked back; a `.yaml` link; a partial role | accepted; equal schema | `schema_round_trips_through_the_schema_argument` and the serde unit tests |
| Old stored schema bytes | serialization unchanged | `index_kind_serialization_is_unchanged` |
| `pl/sort-col-t` (old spelling) | still parses as ascending | `pl_sort_accepts_old_flag_spelling` |

## Test Plan

**Integration test: `liquers-lib/tests/records_table_ops.rs`.** It has `#![cfg(feature =
"records")]`, and polars cases carry `#[cfg(feature = "polars")]`.

- `comparison_filters_parse_to_the_column_type` (AC-1): `gt-amount-50` gives A-1 and A-3; `eq-amount-35` gives A-2; `ne`, `lt`, `lte` and `gte` behave the same way.
- `timestamp_compares_with_an_iso_date` (AC-1): `gte-placed_at-2026~_10~_02` gives A-2 and A-3.
- `eq_value_with_wildcards_is_literal` (AC-1): `eq-customer-Bob*` gives no rows.
- `text_filters_are_case_insensitive_by_default` (AC-2): `starts_with`, `ends_with`, `contains` and `glob` work without the flag; with `-t` the case must match.
- `is_in_and_null_filters` (AC-3): `is_in-order_id-A~_1-A~_3` gives two rows; `is_null-customer` gives A-3; `is_not_null-customer` gives the other two.
- `is_in_without_values_is_refused` (AC-3): `is_in-order_id` is an error.
- `filter_equals_unranked_search` (AC-4): `filter-customer~ncolon~bob` gives the same rows as `ns-search/search-customer~ncolon~bob-f-f-0`.
- `filters_over_a_source_stay_lazy` (AC-5): over `orders_source`, the result identifier is `RecordSource` and no chunk has been streamed yet; it materializes the matching rows.
- `unknown_column_is_refused_naming_available` (AC-6): `eq-nope-1` errors, and the message contains `nope` and `order_id`.
- `sort_orders_stably_with_nulls_first_by_default` (AC-7): ascending, descending, `nulls_last`, and ties keep their input order.
- `sort_over_a_source_materializes_first` (AC-7): it returns a view of all rows.
- `json_list_cells_match_any_element` (AC-8): `eq-tags-b2b` gives A-3; `is_in-tags-gift-bulk` gives A-1 and A-3.
- `shared_commands_have_the_same_arguments` (AC-9, AC-17): for every command name registered in both `rec` and `pl`, the argument names and defaults are equal, except for a listed set of documented differences.
- `rec_and_pl_filters_and_sort_agree` (AC-9, AC-7): with polars, `eq`, `gt`, `contains`, `is_in`, `is_null` and `sort` give the same `order_id` sequence in both namespaces.
- `pl_commands_accept_a_record_view` (AC-10): `ns-pl/sort`, `ns-pl/shape` and `ns-pl/eq` work on `to_record` output.
- `pl_refuses_a_source_naming_materialize` (AC-10): the message contains `ns-rec/materialize`; a Vector column gives the bridge error.
- `pl_sort_accepts_old_flag_spelling` (AC-7, AC-9): `ns-pl/sort-amount-t` sorts ascending.
- `rec_commands_accept_a_dataframe` (AC-11): `-R/data/orders.csv/-/ns-pl/sort-amount/ns-rec/eq-order_id-A~_2` gives one row.
- `tail_and_drop_columns_match_polars` (AC-12): same rows and columns as `ns-pl/tail` and `ns-pl/drop_columns`.
- `drop_columns_refuses_the_id_field` (AC-12).
- `counts_over_a_source_are_chunkwise` (AC-13): `nrows` is 3, `ncols` is 5, `shape` is `{nrows: 3, ncols: 5}`, and `count` gives the non-null counts. `orders_source` streams each chunk once, and no materialized batch exists.
- `shape_and_count_match_polars` (AC-13).
- `schema_and_records_schema_agree` (AC-14): the alias returns the same text.
- `schema_round_trips_through_the_schema_argument` (AC-14): the declared schema of `schema-example.md` is stored as `data/orders.schema.yaml`; `…/to_record-csv-t-~X~-R/data/orders.schema.yaml~E/schema` parses that output with `parse_schema_argument`, and the result equals the declared schema, `FullText` included.
- `table_ops_need_no_polars` (AC-15): a filter, `sort` and `count`, with no polars code involved. Run under `--no-default-features --features records`.
- `select_columns_over_a_source_is_lazy` (AC-18): the result is a `RecordSource`; the materialized batch has `amount` and `order_id` only.
- `head_over_a_source_stops_early` (AC-18): `head-1` over three chunks is a source; materializing it streams only the first chunk.

**Unit tests:**
- `liquers-records/src/column.rs` `sort_indices_is_stable_with_nan_and_nulls` (AC-7) and `sort_indices_refuses_binary_and_vector` (AC-7).
- `liquers-records/src/sources.rs` `mapped_source_maps_each_chunk_and_its_schema` (AC-18).
- `liquers-records/src/search/predicate.rs` `strict_predicate_refuses_a_missing_field` (AC-6).
- `liquers-records/src/search/syntax.rs` `field_test_constructors_escape_regex_metacharacters` (AC-2): `prefix("a.b")` does not match `axb`.
- `liquers-records/src/schema.rs`:
  - `index_kind_accepts_map_and_tag_forms` (AC-14): JSON, a YAML map, and a YAML `!FullText` tag.
  - `field_role_defaults_omitted_fields` (AC-14).
  - `index_kind_serialization_is_unchanged` (AC-14): JSON output is byte-identical to before.
- `liquers-lib/src/value/simple.rs` `utf8_bytes_convert_to_string` (AC-14): bytes that are not UTF-8 are still refused.
- `liquers-core/src/value.rs` `utf8_bytes_convert_to_string` (AC-14).

**Review checks** (not executable; AC-16 records an investigation): `lazyframe-investigation.md`
states the outcome, and the Phase 5 review confirms that no `LazyFrame` value type was added (AC-16).

**Commands to run:**

```bash
cargo test -p liquers-records --all-features --lib --tests
cargo test -p liquers-lib --lib --tests
cargo test -p liquers-lib --no-default-features --features records --test records_table_ops
cargo test -p liquers-lib --test registry_export
bash scripts/check-build-matrix.sh
```

## Learning Log

- A `-` inside a query value is `~_`, not `~-`. `liquers-validate` rejects `2026~-10~-02` with
  "Can't parse query completely".
- `ns-pl` has no `to_record`. Going back to records from polars is `ns-rec/…` on the DataFrame,
  which is AC-11.
- `Glob("*")` binds to the cheap `Present` node (kernels.rs:265), so `is_null` costs one null-mask
  pass.
