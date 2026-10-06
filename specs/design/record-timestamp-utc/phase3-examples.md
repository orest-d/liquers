# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (`ipc.rs`) | Written schema bytes decode to Timestamp(us, "UTC") with the in-tree reader |
| T2 | unit | An IPC payload with an empty zone (fixture bytes from the current writer, captured before the change) reads with identical values |
| T3 | integration (`records_ipc_polars.rs`) | `view → polars` dtype is `Datetime(us, Some("UTC"))` |
| T4 | integration (`records_parquet_polars.rs`) | Parquet path and IPC path give equal dtypes |
| T5 | integration | polars naive `Datetime` column and `Europe/Prague` column → records with identical `i64` values |

Names: `ipc_timestamp_declares_utc`, `ipc_reads_naive_timestamp_as_utc`,
`polars_bridge_timestamp_is_utc`, `timestamp_dtype_agrees_across_parquet_and_ipc`,
`polars_zoned_and_naive_datetimes_keep_epoch_values`.
