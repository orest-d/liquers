# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | JS | `throw new liquers.LiquersError("key_not_found", "no such key: a.txt")` from a JsStore `get` → Rust sees `KeyNotFound` |
| T1 | wasm test (`liquers-web/tests/objects_OBJECT.rs`, ERROR section) | `LiquersError::construct("key_not_found", "m", None)` → `error_type() == "key_not_found"`, `message() == "m"`, `key()`/`query()`/`js_class()` are `None` |
| T2 | wasm test | `construct("no_such_type", "m", None)` → `Err`, and the value is a `TypeError` (`dyn_ref::<js_sys::TypeError>()`) |
| T2b | wasm test | `construct("key_not_found", "m", Some("-R/data/a.txt"))` → `query() == Some("-R/data/a.txt")`, `key() == None` |
| T2c | wasm test | `construct("key_not_found", "m", Some(<unparseable>))` → `Err` `TypeError`; pick the text with `liquers-validate --no-registry` so it is a real parse error |
| T3 | wasm test (`store_js_STORE.rs`) | A JsStore delegate that throws a constructed `LiquersError("key_not_found", …)` → `get` returns `ErrorType::KeyNotFound`, not the fallback |
| T4 | stubs | `valid_usage.ts` constructs one with and one without a query; STUBS02 passes |

Run: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` after
`cargo clean`, then `./liquers-web/scripts/check-stubs.sh` after the quickstart build.
