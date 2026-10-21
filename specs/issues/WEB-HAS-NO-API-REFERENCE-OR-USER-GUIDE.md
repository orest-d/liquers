---
id: WEB-HAS-NO-API-REFERENCE-OR-USER-GUIDE
kind: feature
title: liquers-web has no generated API reference, contract reference or user guide
status: draft
priority: P2
complexity: M
area: [web, docs]
design:
created: 2026-10-20
github:
---

## Problem

The only user-facing documentation of `liquers-web` is `liquers-web/README.md` (a quick start, the
store and record-stream sections, the conformance table and known limitations) plus the `.d.ts` that
`wasm-bindgen` generates. There is:

- no generated API reference (TypeDoc or equivalent over the `.d.ts`), and no check that every
  exported name carries a doc comment;
- no contract reference in `specs/reference/` (the `WEB_API_*` documents there describe
  `liquers-axum`'s HTTP API, not the browser binding);
- no user guide in `specs/guides/` for a page author: configuring an environment, registering
  commands, evaluating, values, errors, stores, testing.

`liquers-py` has all three (`specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/PYTHON_GUIDE.md`
with executed examples in `liquers-py/tests/test_GUIDE_examples.py`, and
`PY-API-REFERENCE-NOT-GENERATED` for the generated half), because its owner asked for them.

## Impact

A page author has to read the README and the Rust sources to learn the binding's contract, for
example what `evaluate` rejects with, what a `JsStore` object must return, or which methods
`LiquersError` has. Nothing executes the README's examples, so they can drift silently. Workaround:
the README and the generated `.d.ts`.

## Expected behaviour

The same three documents `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` names, for JavaScript: a
generated API reference with a completeness check, a `specs/reference/` contract document, and a
`specs/guides/` user guide whose examples a test executes (the Node harness or the Playwright e2e
suite). If that guide section lands first, this issue is its application to `liquers-web`.

## Discovery

Project planning for `liquers-web` on 2026-10-20, while checking
`LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION` against HEAD: its verification asks for the section to be
applied to `liquers-web`, and there was no web-side issue recording the missing documents.
`PY-API-REFERENCE-NOT-GENERATED` is the Python counterpart (weak overlap: same kind of gap, separate
crate and toolchain).
