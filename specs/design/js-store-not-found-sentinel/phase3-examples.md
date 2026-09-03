# Phase 3: Examples and Tests

| Case | Expected result |
|---|---|
| Source reproduction | The source issue's failure becomes the stated successful or typed-error outcome. |
| Compatibility/error path | Existing callers retain their documented behaviour and invalid input retains a typed error. |
| Regression boundary | A focused test proves the precise changed contract, not only execution reachability. |

## Test Plan

Add or amend focused tests beside the named implementation or in the named integration suite. Use descriptive single-behaviour test names, `#[tokio::test]` for async store paths, and assertions on error kind or structured fields rather than message parsing.

**Validation commands:** cargo test -p liquers-web --target wasm32-unknown-unknown --test store_js_STORE; cargo test -p liquers-web --target wasm32-unknown-unknown --test store_conformance_CONF.

