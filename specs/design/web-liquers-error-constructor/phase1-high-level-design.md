# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — what JavaScript may set when constructing a
  `LiquersError`.** Type and message only, or also `key`/`query` provenance?
- **Explanation:** The two-argument form is complete, and optional provenance can be added later
  without breaking it.
- **Open questions:**
  1. **Proposed resolution — `new LiquersError(errorType, message)`.** The type name is validated
     by `error_type_from_name`. An unknown name throws a JavaScript `TypeError` at construction
     (no silent downgrade to `general`). `key`/`query` stay Liquers-populated. `jsClass`/`jsStack`
     are left empty: the error was not translated from a JS exception.

## Problem

`LiquersError` (`liquers-web/src/error.rs`) exposes getters (`errorType`, `message`, `query`,
`key`, `position`, `jsClass`, `jsStack`) but no `#[wasm_bindgen(constructor)]`. A page can receive
a structured error and cannot create one. `js_error_to_liquers` / `LiquersError::from_thrown`
already accept a thrown `LiquersError` without degradation, so the receiving half exists. A page
`JsStore` hitting a permission failure can only produce the adapter's fallback type
(`KeyReadError`).

## Expected behaviour and acceptance

1. `throw new liquers.LiquersError("key_not_found", "no such key: a.txt")` from a `JsStore`
   delegate reaches Rust as `ErrorType::KeyNotFound` with that message.
2. `new liquers.LiquersError("no_such_type", "x")` throws `TypeError` naming the bad type.
3. `errorType`/`message` getters return what was passed. `key`, `query`, `jsClass` and `jsStack`
   are null.
4. The TypeScript declaration has `constructor(errorType: string, message: string)` (generated),
   and the stubs test uses it.

## Scope

`liquers-web/src/error.rs`, its tests, the stubs usage file, and the README. Core is untouched.

## Design Dependencies

- `configuration-error-kind` — **overlaps**. A new `ErrorType` name becomes constructible
  automatically through `error_type_from_name`.
- `web-object06-error-type-exhaustiveness` (P2, ready) — **overlaps** (same test file).

## Documentation assessment

`liquers-web/README.md` (error section: how a page raises a typed error).
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`, if it describes the error bridge contract.

## Consolidated Findings

- A wasm-bindgen class is not a subclass of JavaScript `Error`, so a thrown `LiquersError` carries
  no JS stack. That is acceptable, because the bridge checks for the class first. Document it.
- The Rust constructor must not be named `new`, which is taken by `LiquersError::new(inner: Error)`.
