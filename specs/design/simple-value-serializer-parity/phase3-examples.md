# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (existing, tightened) | `every_declared_format_round_trips_or_is_recorded_as_unwritable` renamed `every_declared_format_round_trips`; no exceptions |
| T2 | unit | `SimpleValue::Bytes(vec![0,159]).as_bytes("bin") == [0,159]` |
| T3 | unit | `SimpleValue` query `-R/a/b.txt` as `txt` equals core `Value::Query(..).as_bytes("txt")` |
| T4 | unit | parity: for every declared pair, `SimpleValue` bytes == core `Value` bytes for the corresponding sample |

T4 needs a mapping from `SimpleValue` samples to core `Value` samples. Build both sample sets side
by side in the test. Run with `cargo test -p liquers-lib --lib simple`.
