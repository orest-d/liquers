# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | HTTP | Store with `data/a.txt`, `data/sub/b.txt`; `GET /api/store/keys?prefix=data` contains `data/sub/b.txt` |
| T2 | HTTP | `GET /api/store/keys` contains both nested keys |
| T3 | regression | `GET /api/store/listdir/data` still lists only direct children |

In `liquers-axum/tests/store_api_routes.rs`, with the suite's memory-store fixture. Names:
`store_keys_lists_nested_keys_under_prefix`, `store_keys_without_prefix_lists_whole_store`.
