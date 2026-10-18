---
id: WEB-OUTBOUND-INT64-PRECISION
kind: design
title: Pin what an i64 beyond the safe-integer range does when it leaves for JavaScript
form: compact
status: complete
gh_pr: [154]
readiness: ready
autofix: eligible
area: [web]
issues: [WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED]
created: 2026-10-17
---
# Pin what an i64 beyond the safe-integer range does when it leaves for JavaScript

Bulk design (`specs/guides/autonomous_bulk_design.md`). Implemented: the outbound test and the
`serialize_to_js` doc paragraph are in place, and the issue is closed.

## Phase 1: High-Level Design

### Purpose

`VALUE03` (`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE, blueprint
`test_VALUE03_integer_boundaries`) requires every integer to round-trip exactly or raise
`ConversionError`, never to lose precision silently. `liquers-web` tests only the inbound half
(`liquers-web/tests/value_bridge_VALUE.rs` `value03_integer_boundaries`). The outbound half —
`liquers-web/src/bridge.rs` `value_to_js` → `try_into_json_value` → `json_to_js` →
`serialize_to_js` — has no test, and the audit could not say what it does.

This design establishes the behaviour from the dependency's source and pins it.
`serialize_to_js` builds `serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true)`,
leaving `serialize_large_number_types_as_bigints` at `false`. In serde-wasm-bindgen 0.6
(`src/ser.rs` `serialize_i64` / `serialize_u64`, read from the 0.6.5 crate) that mode emits a
`number` only inside `Number.MIN_SAFE_INTEGER..=Number.MAX_SAFE_INTEGER` (±(2^53 − 1)) and
otherwise returns `Err("<n> can't be represented as a JavaScript number")`. `json_to_js` maps that
error to `ErrorType::ConversionError`. No workspace crate enables `serde_json/arbitrary_precision`,
which would change the path. So today's behaviour already satisfies `VALUE03`: it raises. What is
missing is a test and a sentence saying so.

### Problem Example

A Rust command returns `Value::from_i64(9_007_199_254_740_993)` (2^53 + 1, an id). A page calls
`await env.evaluate("…")`.

- **Today (established from source, unpinned):** the promise rejects with a `ConversionError`
  "Could not convert a Liquers value to JavaScript: 9007199254740993 can't be represented as a
  JavaScript number". Nothing in the suite would notice if a dependency bump, a serializer option
  or a hand-written fast path made it resolve to the imprecise `9007199254740992` instead.
- **Should:** the same rejection, asserted by a test; `9_007_199_254_740_991` (2^53 − 1) arrives as
  that exact `number`.

### Scope and Acceptance Criteria

- **AC-1** Safe integers arrive exactly
  - WHEN `value_to_js` converts `Value` integers `0`, `2^31`, `2^53 − 1` and `−(2^53 − 1)`
  - THEN each arrives as a JavaScript `number` equal to the integer
- **AC-2** Unsafe integers are refused, not rounded
  - WHEN `value_to_js` converts `2^53`, `2^53 + 1`, `i64::MAX` or `i64::MIN`
  - THEN it returns an error with `ErrorType::ConversionError` whose message contains the integer
- **AC-3** The refusal reaches nested integers too
  - WHEN a JSON-shaped value `{"id": 2^53 + 1}` or `[2^53 + 1]` is converted
  - THEN the whole conversion fails with `ConversionError` rather than producing a container with an
    imprecise element
- **AC-4** The policy is written down
  - WHEN a reader opens `serialize_to_js`'s doc comment
  - THEN it states that integers beyond ±(2^53 − 1) are refused rather than sent as `BigInt`, and
    names `VALUE03`

Non-goals: changing the policy to `BigInt` output (see Phase 2, rejected alternatives); the inbound
direction, already tested.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — new tests in `liquers-web/tests/value_bridge_VALUE.rs` and a doc
  comment in `liquers-web/src/bridge.rs`; no behaviour, binding or signature changes; one crate.
- **Leading issue:** None
- **Explanation:** The current behaviour already meets `VALUE03` by raising; only its pinning is
  missing. Choosing `BigInt` instead would change what pages receive, and is not needed for
  conformance.
- **Open questions:** None. `BigInt` output stays possible later as a feature, and the test written
  here would then change deliberately.

### Design Dependencies

- `overlaps` `WEB-DOCS-STALE-AFTER-KEYED-EVAL-FIX` (its design edits the module comment of
  `value_bridge_VALUE.rs`, lines 15-17); different lines, no ordering.

### Consolidated Findings

- The established behaviour depends on serde-wasm-bindgen's default; the new test is what holds it
  now. A future `serialize_large_number_types_as_bigints(true)` makes AC-1/AC-2 fail on purpose.
- **Asymmetry, not a defect:** inbound `number_to_value` (`bridge.rs`) accepts `|n| ≤ 2^53` as an
  integer, outbound refuses `2^53`. So a JS `9007199254740992` enters as an `I64` and cannot be sent
  back. `VALUE03` allows raising, so this conforms; the test comments it so nobody "fixes" one side
  alone.
- Run under Node; no browser needed.

## Phase 2: Architecture

### Solution

Keep the policy (refuse outside the safe range, through serde-wasm-bindgen's own check). Add one
test function and a doc-comment paragraph. Rejected:

- **Serialize large integers as `BigInt`** (`serialize_large_number_types_as_bigints(true)`): changes
  the binding — every `u64`/`i64` field the serializer touches, including asset-info sizes in
  `asset.rs`, would become `BigInt` when large, and pages mixing `number` and `BigInt` arithmetic
  throw `TypeError`. Not required by `VALUE03`.
- **A liquers-web range check before serialization:** duplicates the dependency's check with no gain.

Known-issue preflight: no open issue or design touches `serialize_to_js` or `value_to_js`
(`liquers-web`, `liquers-web-store` and `keyed-recipe-ownership` designs are complete).

### Changes

- `liquers-web/tests/value_bridge_VALUE.rs`: new `#[wasm_bindgen_test] fn
  value03_outbound_integer_boundaries()` beside `value03_integer_boundaries`, using `value_to_js`
  on `Value::from_i64(n)` for scalars and on `Value::try_from_json_value(&serde_json::json!(…))`
  for containers (`ValueInterface`, `liquers-core/src/value.rs`).
- `liquers-web/src/bridge.rs` `serialize_to_js`: doc paragraph for AC-4. No code change.

### Risks

Only a test and a comment change. If the test fails, the established behaviour is wrong: stop and
re-file as a `VALUE03` defect (rollback in Phase 4). Certainty: high (source read; not executed —
no wasm runner in the design environment).

## Phase 3: Examples and Tests

### Examples

The Problem Example. For AC-3: `{"id": 9007199254740993}` → `Err(ConversionError)`.

### Tests

- `value03_outbound_integer_boundaries` (new): AC-1 (four safe values compare equal with
  `as_f64()`), AC-2 (four unsafe values give `ConversionError`, message contains the decimal
  integer), AC-3 (object and array cases)
- AC-4 is checked by review of the doc comment in step 2.

Run: `cargo test -p liquers-web --target wasm32-unknown-unknown --test value_bridge_VALUE`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-web/tests/value_bridge_VALUE.rs` — add `value03_outbound_integer_boundaries` (AC-1..3), with a comment on the inbound/outbound `2^53` asymmetry — `cargo test -p liquers-web --target wasm32-unknown-unknown --test value_bridge_VALUE`. Rollback: if AC-2 fails (a value arrives as a `number`), stop, keep the test as the reproduction, and re-label this design `not-eligible` with the policy as a needs-decision question.
- [x] 2. `liquers-web/src/bridge.rs` `serialize_to_js` — doc paragraph: integers outside ±(2^53 − 1) are refused with `ConversionError`, not sent as `BigInt` (`VALUE03`) — `cargo check -p liquers-web --target wasm32-unknown-unknown`
- [x] 3. Close `WEB-OUTBOUND-INT64-BEYOND-DOUBLE-UNTESTED` with a resolution note — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`

### Validation

The test command above and `cargo test -p liquers-web --target wasm32-unknown-unknown --features
debug-handles` (the full Node loop) pass; `python3 scripts/docs_index.py --check`.
