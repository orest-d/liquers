---
id: USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED
kind: issue
title: User guide chapter examples are not executed
status: draft
priority: P2
complexity: M
area: [docs, build]
design: 
created: 2026-11-01
github:
---
## Problem

The user guides' code examples are not run by anything. Chapter 1's hello-world is copied from a
test that runs it (`liquers-core/tests/async_hellow_world.rs`, `liquers-py/tests/test_GUIDE_examples.py`,
`liquers-web/tests/guide/run-guide-examples.mjs`) with an `EDITOR:` note to keep the two
identical, but nothing checks that they are. Later chapters will add many more examples.

## Impact

An example can stop working without any test failing — the documentation counterpart of a hollow
conformance test (`LANGUAGE-INTEGRATION_GUIDE.md` §DOCS). Low today (one example per guide), high
once chapters are drafted.

## Expected behaviour

The examples of `docs/guides/<guide>/` run in the routine loop of their language, the way
`test_GUIDE_examples.py` runs `PYTHON_GUIDE.md`'s: Rust blocks as doctests or an extracted test,
Python through pytest, JavaScript through the Node runner. An example that cannot run is marked,
with the reason.

## Discovery

`design/user-documentation-guides` Phase 3 deferred it ("running chapter examples directly is a
later step"); filed at Phase 5.
