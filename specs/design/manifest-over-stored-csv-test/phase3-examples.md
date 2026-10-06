# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | integration | `materialize` returns 5 rows: jan rows then feb rows; column order `month, amount` |
| T2 | integration | After T1, `store.contains` is false for any key other than the three written files and the manifest (chunks unkeyed) |
| T3 | integration | A variant manifest including `bad.csv` (`amount` empty in a non-nullable column) → error; message names the chunk query or index |
| T4 | integration | Re-evaluating the materialize query returns the same 5 rows (rewindable source) |

Names: `manifest_over_stored_csv_files_materializes_in_order`,
`manifest_csv_chunks_are_unkeyed`, `manifest_csv_chunk_violating_uniform_schema_fails`.
Run: `cargo test -p liquers-lib --test records_manifest_over_csv_files`.
