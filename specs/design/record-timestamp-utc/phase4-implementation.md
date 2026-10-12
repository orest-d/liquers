# Phase 4: Implementation Plan

Precondition: question 1 accepted.

1. Capture a pre-change IPC fixture (bytes of a 1-row Timestamp table) for T2. Commit it in the
   test as a constant.
2. IPC writer + reader doc (Phase 2). Proof: T1, T2,
   `cargo test -p liquers-records --all-features --lib ipc`. Agent: sonnet tier; knowledge: Arrow
   `Schema.fbs` Timestamp table.
3. Polars bridge casts. Proof: T3–T5, `cargo test -p liquers-lib --test records_ipc_polars --test records_parquet_polars`.
4. `bash scripts/check-build-matrix.sh`.
5. Reference table + decision note, issue resolution, index. Diff review.

## Outcome (2026-10-11)

Steps 1–5 done; T1–T5 pass, plus `ipc_reads_a_timestamp_of_any_zone`. Two differences from
Phase 2:

- **polars `timezones` feature.** Without it polars' `cast` drops a time zone, and its Parquet
  reader reports `isAdjustedToUTC` as zone `+00:00`, so T4 could not hold. `liquers-lib` enables
  it, and the bridge sets the logical type with `Series::into_datetime` rather than `cast`.
- **Reading keeps the input's zone.** Polars → records casts only the unit, keeping the input's
  zone, rather than casting to UTC, so no zone conversion can shift a value.
