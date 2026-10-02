---
id: LOCAL-STORAGE-STORE-FAILS-CONFORMANCE-IN-A-BROWSER
kind: issue
title: LocalStorageStore fails three conformance rules, and its conformance test never runs in a browser
status: closed
priority: P2
complexity: M
area: [web, core/store]
design: store-conformance-backlog
created: 2026-09-27
github:
---
# `LocalStorageStore` fails three conformance rules, and its conformance test never runs in a browser

## Problem

`c9_local_storage_store` (`liquers-web/tests/store_conformance_CONF.rs`) is gated on the
`browser-tests` feature. Its file carries no `wasm_bindgen_test_configure!(run_in_browser)`, so
`cargo test … --features browser-tests` still runs it **under Node**. There `web_sys::window()` is
`None`, and the test panics at `expect("a browser window")` before checking anything.

Forced into a browser (`WASM_BINDGEN_USE_BROWSER=1`, driven over Playwright), it reaches the suite
and fails three rules:

- **absence03** (`STORE_SEMANTICS.md` §4): `removedir` on an absent directory returns `KeyNotFound`.
  The postcondition already holds, so it should return `Ok(())`.
- **keyshape01** (§7): `contains` accepts the relative key `data/../../escape.txt`. A store never
  resolves a relative key.
- **keys02** (§9): `keys()` omits directories above the stored data keys. It returns
  `…-n24/d0/leaf.txt` but neither `…-n24/d0` nor `…-n24`.

## Expected behaviour

The test runs in a browser whenever `browser-tests` is on, for example by moving it into a file
that configures `run_in_browser`, as `store_local_STORE.rs` does. `LocalStorageStore` then passes
all three rules.

## Discovery

Found 2026-09-27 in record-streams Step 8.3, running the browser loop through the README's
`NO_HEADLESS=1` route. The container's chromedriver (147) does not match its Chromium (141). The
test file is unchanged by `record-streams`.

## Resolution

Closed 2026-09-30 by `design/store-conformance-backlog/` step 8 (`0f040e7`). `removedir` on an
absent directory is `Ok(())`, `contains`/`is_dir`/`listdir` refuse a relative key, and `keys()`
returns data keys, their directories and the prefix. A metadata-only key is also indexed, which
the new rule `sidecar04` requires. C9 moved to `tests/store_conformance_browser_CONF.rs`, which
configures `run_in_browser`. It was run through the README's `NO_HEADLESS=1` route with Playwright's
Chromium 141 (the container's chromedriver is 147): 30 of 43 rules, 0 failed.
`store_local_STORE`: 11/11.
