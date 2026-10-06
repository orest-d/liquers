# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (`store.rs`) | Memory: `set_metadata` on a new key → `contains` true, `get_bytes` `KeyNotFound`, `get` `KeyNotFound` |
| T2 | unit | Memory: `set(k, b"", m)` → `get_bytes == []` |
| T3 | unit | Memory: `set` then `set_metadata` keeps data |
| T4 | conformance | `sidecar05` on every suite in `tests/store_conformance_CONF.rs` |
| T5 | unit (`assets.rs`) | Part G: a metadata-only memory entry with a content-hash version is skipped (no longer misread as changed outside Liquers) |
| T6 | unit | Part G: an empty data object with a timestamp version is checked (formerly skipped by the heuristic) |

Commands to run: `cargo test -p liquers-core --lib store`,
`cargo test -p liquers-core --test store_conformance_CONF`,
`cargo test -p liquers-core --lib verify_stored`, and the browser suites per CLAUDE.md for
`JsStore`/`LocalStorageStore`.
