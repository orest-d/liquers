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
