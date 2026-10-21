# Phase 3: Examples & Use-cases — value-summary

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A table's metadata answers "how big?" | the client view | AC-2, AC-5 |
| 2 | Example | A downstream extension summarizes itself | extensibility | AC-7 |
| 3 | Test | `liquers_core::state::tests::summary_follows_the_value` | replace value, cleared for scalars | AC-3, AC-4 |
| 4 | Test | `liquers_core::metadata::tests::value_summary_is_optional_in_json` | absent key, old record loads, round trip | AC-6 |
| 5 | Test | `liquers-lib/tests/value_summary.rs` `image_summary_has_size` | image | AC-1 |
| 6 | Test | `liquers-lib/tests/value_summary.rs` `dataframe_summary_has_shape` (`polars`) | frame | AC-2 |
| 7 | Test | `liquers-lib/tests/value_summary.rs` `record_view_summary_has_shape` (`records`) | view | AC-2 |
| 8 | Test | `liquers-lib/tests/value_summary.rs` `summary_survives_store_and_listing` | `set_state`, `get_metadata`, `listdir_asset_info` | AC-5 |
| 9 | Test | `liquers-lib/tests/value_summary.rs` `extension_summary_reaches_metadata_json` | a test `ValueExtension` overriding the hook | AC-7 |

## Example 1: a table's metadata

```rust
let state = State::new().with_data(Value::from_record_view(Arc::new(batch))); // 2 rows: region, amount
let record = state.metadata.metadata_record().ok_or("record")?;
let s = record.value_summary.as_ref().ok_or("summary")?;
assert_eq!(s.properties["rows"], json!(2));
assert_eq!(s.properties["fields"], json!(["region", "amount"]));
```

Stored under `data/sales/table.csv` and listed with `listdir_asset_info(&parse_key("data/sales")?)`,
the `AssetInfo` for `table.csv` carries the same `value_summary`.

## Example 2: a downstream extension

```rust
impl ValueExtension for MatrixExt {
    fn value_summary(&self) -> Option<ValueSummary> {
        match self {
            MatrixExt::Matrix(m) => Some(ValueSummary::new(format!("{}×{} matrix", m.rows, m.cols))
                .with("rows", m.rows).with("cols", m.cols)),
            MatrixExt::Foreign(_) => None,
        }
    }
    // …
}
```

## Edge and Error Cases

- **Value replaced by a scalar**: summary cleared, not kept from the previous value (test 3, AC-4).
- **Legacy metadata** (`Metadata::LegacyMetadata`): `with_value_summary` inserts or removes the key
  in the JSON object (test 4).
- **Record written before this change**: loads with `None` (test 4, AC-6).
- **Empty frame**: `rows: 0`, fields listed (test 6).
- **`RecordSource`**: no summary (test 7 asserts `None` for a manifest source).
- **Feature-off builds**: gated tests (`#[cfg(feature = …)]` per test).

## Test Plan

- Unit: `liquers-core/src/state.rs` `summary_follows_the_value` (AC-3, AC-4): `Text("abc")` → `bytes: 3`;
  `with_data(Value::from(1i64))` → `None`; `Array` of 2 → `len: 2`.
- Unit: `liquers-core/src/metadata.rs` `value_summary_is_optional_in_json` (AC-6): a record without
  the field serializes without `value_summary`; a fixture JSON lacking it deserializes as
  `MetadataRecord`; one with it round-trips equal.
- Integration: `liquers-lib/tests/value_summary.rs` — tests 5-9 above; test 8 uses
  `DefaultEnvironment<Value>` with an `AsyncMemoryStore`, `set_state` on `data/sales/table.csv`,
  then `get_metadata` and `listdir_asset_info`.
- Regression: `cargo test -p liquers-core`; `cargo test -p liquers-lib --lib --tests`;
  `cargo test -p liquers-axum` (metadata JSON assertions); `bash scripts/check-build-matrix.sh`.

## Learning Log

- `liquers-axum` already uses the name `ValueDescription` for something else; the feature's name
  had to change to avoid two meanings in one HTTP surface.
