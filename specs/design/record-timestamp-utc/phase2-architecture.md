# Phase 2: Solution and Architecture

## IPC (`liquers-records/src/formats/ipc.rs`)

- `write_timestamp_type(builder)` writes `unit = MICROSECOND`, `timezone = "UTC"` (create the
  string before starting the table, as FlatBuffers requires).
- The reader's Timestamp type decoding ignores the zone for value purposes. It accepts `None`,
  `"UTC"` and any other zone. Update the module doc comment ("timezone(1, unused…)").

## Polars bridge (`liquers-lib/src/records/polars.rs`)

- Records → polars: cast to `DataType::Datetime(TimeUnit::Microseconds, Some("UTC".into()))` at
  both sites. Check the polars version's `TimeZone` type for the constructor spelling
  (`Some(TimeZone::UTC)` vs a `PlSmallStr`).
- Polars → records: the arm `DataType::Datetime(_, _) => FieldType::Timestamp` already accepts any
  zone. Values are read in microseconds after `cast` to the UTC-zoned type, which does not shift
  the underlying epoch values. Verify that with a test (acceptance 4), because polars' casting
  between zones keeps the physical value.

## Known-issue preflight

None.

## Relevant commands

`ns-pl` conversions from records, `ns-rec` reading `.arrow`/`.ipc` (feature `records-ipc`).

## Documentation architecture

RECORD_STREAMS.md type table and decision note.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `formats/ipc.rs`, `records/polars.rs`, `tests/records_ipc_polars.rs`, `tests/records_parquet_polars.rs` |
| Feature gates | `records-ipc`, `polars`. Run the matrix. |
| Existing tests | Tests asserting `Datetime(us, None)` change |
| Data | New IPC files carry `UTC`. Old files read unchanged. |
| Interop | pyarrow/pandas see tz-aware UTC timestamps. That is the intended fix, and it matches Parquet. |
| Recovery | Revert |
| Certainty | High once decided |
