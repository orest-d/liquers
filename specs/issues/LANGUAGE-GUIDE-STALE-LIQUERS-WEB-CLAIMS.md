---
id: LANGUAGE-GUIDE-STALE-LIQUERS-WEB-CLAIMS
kind: issue
title: LANGUAGE-INTEGRATION_GUIDE makes stale claims about liquers-web
status: draft
priority: P2
complexity: S
area: [docs, web]
design: language-guide-web-claims
created: 2026-10-17
github:
---

## Problem

Four statements in `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (reviewed 2026-10-16) do not match
the implementation or the guide's own appendix:

1. **Harness claim (L106-109):** "all thirteen `VALUE` tests for `liquers-web` … run under Node …
   while `RUNTIME`, `ASYNCQ` and `PACKAGE` need a real browser." Only
   `tests/store_local_STORE.rs` and `tests/store_conformance_browser_CONF.rs` call
   `wasm_bindgen_test_configure!(run_in_browser)` (behind the `browser-tests` feature);
   `runtime_RUNTIME.rs` and `async_ASYNCQ.rs` run under Node. Only the Playwright `PACKAGE`/e2e
   specs need a browser.
2. **OBJECT09 one-liner (L271):** still says a host string that cannot be represented "raises";
   the 2026-08-14 History entry and the Appendix A blueprint (L1730-1739) redefined it as a
   round-trip test, and `encode_token` is now total. §5 was never updated.
3. **Rebuild trigger (L621-623):** "reconstructs its environment … on command registration" is
   incomplete — `configureStore`, `registerStoreObject` and `unregisterCommand` after sharing also
   rebuild (`liquers-web/src/environment.rs:343`, `:368`, `:447`).
4. **"full 83-test disposition" (L1548):** cited as the worked example's inventory; the prescribed
   inventory has since grown (`ENVIRON08`, `RECORDS01`-`11`, `STORE14`-`18`), so the figure no
   longer describes conformance with the current guide.

## Impact

An integration author following the guide plans a browser harness for suites that run under Node,
writes OBJECT09 against the superseded contract, and treats the worked example as currently
conformant. Documentation only.

## Expected behaviour

Correct the four statements (with a `## History` row and a `reviewed:` bump per
`DOCS_STRUCTURE_GUIDE.md` §9.2). The stale harness claim also appears in
`liquers-web/tests/value_bridge_VALUE.rs:15-17` (tracked in `WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX`).

## Discovery

Bidirectional conformance audit (2026-10-17): the guide's descriptive claims about liquers-web
were fact-checked against the code.
