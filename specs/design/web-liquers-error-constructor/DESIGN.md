---
id: ACTIVE-12
kind: design
title: Design for WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE
status: in_review
phase: implementation
readiness: needs-decision
area: [web, core/error]
issues: [WEB-LIQUERSERROR-NOT-CONSTRUCTIBLE]
created: 2026-09-03
---

# Design Tracking

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

## Review 2026-10-06

Re-verified against HEAD after the latest merges. **Rewritten (the original phases were template text), with
two corrections:** the test file the plan named (`liquers-web/tests/error_ERROR.rs`) does not
exist (the ERROR tests live in `objects_OBJECT.rs`), and `LiquersError` already has a Rust
associated function `new(inner: Error)`, so the wasm constructor needs a different Rust name with
`#[wasm_bindgen(constructor)]`. `LiquersError` also gained `jsClass`/`jsStack` getters
(`LANGUAGE-EXCEPTION-FIELDS-LOST-IN-TRANSPORT`), which a page-constructed error leaves empty.
Readiness stays `needs-decision` (constructor arguments).
