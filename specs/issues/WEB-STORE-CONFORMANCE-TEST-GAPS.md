---
id: WEB-STORE-CONFORMANCE-TEST-GAPS
kind: issue
title: liquers-web's STORE tests are narrower than the guide's prescribed contracts
status: closed
priority: P3
complexity: M
area: [web, core/store]
design: web-store-conformance-tests
created: 2026-10-17
github:
---

## Problem

Five gaps between `LANGUAGE-INTEGRATION_GUIDE.md`'s STORE inventory and the liquers-web suites:

1. **`STORE05b`** (the ENOENT trap: after `makedir("a")`, `a/../../etc` still gives
   `KeyNotAbsolute`) has no liquers-web test; the post-makedir variant exists only natively as
   `keyabs08` (`liquers-core/src/store.rs:3509`). The dotted negatives the blueprint requires
   (`d/.hidden`, `d/a..b`, `d/...`, `d/..x`, `d/v1.2.3`) appear in no liquers-web test either
   (core tests them in `liquers-core/tests/store_key_absolute.rs`).
2. **`STORE11`'s overlap half**: "overlapping prefixes: first in document order wins
   (`data` shadows `data/scratch`)" is untested from a configuration document anywhere; the
   liquers-web STORE11 tests use disjoint prefixes (`store_js_STORE.rs:277`, `e2e/store.spec.ts`).
   Core's `routerfwd10`/`routerfwd11` cover router-level overlap but not document order. The
   router also warns about nothing on shadowing, and the design question "does the integration
   warn?" has no recorded answer. (Already noted as "the one genuine gap" in
   `specs/design/liquers-web-store/DESIGN.md`.)
3. **`STORE14`** tests only `set` (not "every write": `set_metadata`, `remove`, `removedir`,
   `makedir`), and checks read-only through the Rust trait rather than the JavaScript
   `LiquersStore.isReadOnly()` (`store_js_STORE.rs:771-786`).
4. **`STORE17`** sends no explicit `precision` field from `dataFacts`, so the documented
   `millis` conversion path through `TimePrecision::from_name` is untested
   (`store_js_STORE.rs:858-876`).
5. **`STORE18`** has no test and no recorded `NA`. It is legitimately `NA` — a page cannot define
   a `store_config` handler — but the disposition linking
   `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` is written nowhere.

## Impact

The STORE feature's conformance claim is weaker than it looks: the contracts are implemented (core
tests cover most of the behaviour) but the integration-level tests do not pin them, so a
regression in the wasm path would pass the suite. Documentation/disposition gaps (2, 5) violate
the guide's "absence is not an `NA` decision".

## Expected behaviour

Add the missing test cases under their guide IDs in the existing suites
(`store_pure_STORE.rs`, `store_js_STORE.rs`), a config-document overlap-order test, and record the
`STORE18` `NA` with its reversing condition wherever the conformance disposition lives.

## Discovery

Bidirectional conformance audit (2026-10-17), comparing the guide's Appendix A STORE blueprints
against `liquers-web/tests/`.

## Resolution (2026-10-18)

All six gaps are covered in liquers-web's suites: dotted names in `keyguard04` and in
`store05b_traversal_after_makedir_is_refused` (Node, and a browser twin in `store_local_STORE.rs`
gated behind `browser-tests`), `store11_overlapping_prefixes_follow_document_order`, a
`store14_read_only_js_store_refuses_writes` covering every write over a writable page object plus
`Store.isReadOnly()`, and `store17_explicit_precision_is_applied` /
`store17_unknown_precision_is_refused`. `liquers-web/README.md` `## Conformance` records `STORE18` as
`NA` and that overlapping prefixes are accepted silently. Design:
`specs/design/web-store-conformance-tests/`.
