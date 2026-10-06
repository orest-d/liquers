# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit, both managers | `set_state` with supplied `Expired` → `expiry_reason() == Some(Direct{Explicit})`, stored and live |
| T2 | unit, both managers | `set_binary` same |
| T3 | unit | Supplied `Expired` with `Cascaded{…}` reason → kept |
| T4 | unit | Supplied `Ready` → no reason |

Setup: `AsyncMemoryStore`, key `data/x.txt`, a `Text` value, metadata status set with
`metadata.set_status(Status::Expired)`. Read back with `store.get_metadata(&key)` and
`manager.get_metadata(&key)` (or the live asset's `get_metadata`).

Names: `supplied_expired_state_records_explicit_reason`,
`supplied_expired_binary_records_explicit_reason`, `supplied_expiry_reason_is_kept`,
`supplied_ready_has_no_expiry_reason`.
