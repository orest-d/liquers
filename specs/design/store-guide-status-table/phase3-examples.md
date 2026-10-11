# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | run | `cargo test -p liquers-core --test store_conformance_CONF -- --ignored --nocapture status_table` prints a table whose native rows equal the guide's current rows (counts and statuses) |
| T1b | run | the same in `liquers-store` (`--features store-conformance`) prints the two OpenDAL rows, equal to the guide's |
| T2 | regression | `cargo test -p liquers-core --test store_conformance_CONF` and `cargo test -p liquers-store --features store-conformance --test store_conformance_CONF` unchanged |
| T3 | doc | Guide §9 regenerated from T1 output. Browser rows unchanged. |

If T1 disagrees with the guide, the guide is stale. The PR updates it from T1 and says which rows
changed.
