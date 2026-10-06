# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | JS | `await store.set("input.csv", bytes, {}); (await store.getAssetInfo("input.csv")).media_type === "text/csv"` |
| T1 | wasm test (Node loop) | Rust-side test in `liquers-web/tests/` calling the wrapper over a memory-backed store: `media_type == "text/csv"` |
| T2 | wasm test | Directory key → `is_dir == true` |
| T3 | wasm test | Absent key → rejected with `KeyNotFound` |
| T4 | stubs | `check-stubs.sh` STUBS02 compiles the new usage line |
| T5 | e2e (optional) | Playwright e2e test asserting E1 in the quickstart page, replacing the workaround that motivated the issue if one exists |

Run `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` after
`cargo clean`.
