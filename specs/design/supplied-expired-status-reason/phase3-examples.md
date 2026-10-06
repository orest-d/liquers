# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit, both managers | `set_state` with supplied `Expired`, no reason → stored log ends with warning `"Asset expired"` then an info containing `"after the fact"` and `"unknown"`; `expiry_reason()` is `None` |
| T2 | unit, both managers | `set_binary`, same, with route `set_binary` in the info |
| T3 | unit | Supplied `Expired` with a `Cascaded{…}` reason → reason kept, info names it |
| T4 | unit | Supplied `Ready` → no new log entries |

Setup: `AsyncMemoryStore`, key `data/x.txt`, a `Text` value, `metadata.set_status(Status::Expired)`.
Read back with `store.get_metadata(&key)`.
Names: `supplied_expired_state_logs_asset_expired`, `supplied_expired_binary_logs_asset_expired`,
`supplied_expiry_reason_is_kept_and_logged`, `supplied_ready_adds_no_expiry_log`.
