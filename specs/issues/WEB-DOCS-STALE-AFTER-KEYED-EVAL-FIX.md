---
id: WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX
kind: issue
title: liquers-web documentation still describes limitations that were fixed
status: closed
priority: P2
complexity: S
area: [web, docs]
design: web-docs-stale-limitations
created: 2026-10-17
github:
---

## Problem

`liquers-web/README.md` "Known limitations" is stale on two counts, and three smaller comments
rotted with it:

- README line ~226 says "**`-R/` queries do not evaluate in the browser yet**" and that the blocked
  e2e tests "are kept as `fixme`". Both P1 blockers are closed
  (`CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`, `IMMEDIATE-MANAGER-NO-FAST-TRACK`, fixed by
  `specs/design/keyed-recipe-ownership/`), and `tests/e2e/store.spec.ts` no longer contains any
  `fixme` — the `STORE07`/`STORE11` cases run.
- README line ~236 says `encodeParam` "refuses values containing a lone colon, most punctuation, or
  any non-ASCII character". `PARAMETER-ESCAPING-INCOMPLETE` is closed; `liquers-web/src/encode.rs`
  now delegates to the total core encoder and its own doc says the limitation was removed.
- `liquers-web/Cargo.toml:18-21` carries a comment describing the removed `liquers-store`
  dependency.
- `liquers-web/src/store/mod.rs:6` says "Four stores" above a table listing three.
- `liquers-web/tests/value_bridge_VALUE.rs:15-17` claims RUNTIME and ASYNCQ suites need a real
  browser; they run under Node (no `wasm_bindgen_test_configure!(run_in_browser)` in those files).

## Impact

A reader (or agent) plans around limitations that no longer exist — the first one hides the
crate's headline capability, resource queries in a page. No behavioural impact.

## Expected behaviour

Update the README Known-limitations section to the current state (the remaining real limitations
are `WEB-CANCELLATION-INERT`, `POST-INIT-COMMAND-REGISTRATION`,
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`), and fix the three stale comments.

## Discovery

Bidirectional conformance audit of `liquers-web` against `LANGUAGE-INTEGRATION_GUIDE.md`
(2026-10-17): the issue index says closed, the docs say open.

## Resolution (2026-10-18)

Fixed by `specs/design/web-docs-stale-limitations/`. `liquers-web/README.md` § Known limitations no
longer lists `-R/` evaluation or the `encodeParam` refusal; the four remaining rows name open issues
(`WEB-CANCELLATION-INERT`, `POST-INIT-COMMAND-REGISTRATION`,
`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`). The orphaned `Cargo.toml` comment is removed,
`src/store/mod.rs` says "Three stores", and the harness note in `tests/value_bridge_VALUE.rs` names
the suites that actually need a browser.
