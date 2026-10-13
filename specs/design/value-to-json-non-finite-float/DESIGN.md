---
id: VALUE-TO-JSON-NON-FINITE-FLOAT
kind: design
title: Converting a non-finite float to JSON returns an error instead of panicking
form: compact
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value, py]
issues: [VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT]
created: 2026-10-13
---
# Converting a non-finite float to JSON returns an error instead of panicking

## Phase 1: High-Level Design

### Purpose

`try_into_json_value` must never panic. A float that JSON cannot represent (NaN, +∞, −∞) is
refused with a `ConversionError`, the same answer `liquers-records`' JSON writer already gives for
the same value, so one computed NaN can no longer take down an evaluation, an HTTP request or a
Python interpreter.

### Problem Example

```rust
use liquers_core::value::{Value, ValueInterface};

let v = Value::from_f64(f64::NAN);          // accepted: from_f64 has no finiteness check
let json = v.try_into_json_value();         // today: panics in serde_json::Number::from_f64(..).unwrap()
```

**Today** the call panics (`called Option::unwrap() on a None value`,
`liquers-core/src/value.rs` `impl ValueInterface for Value::try_into_json_value`, the `Value::F64`
arm). The same `unwrap` exists verbatim in two other implementations of the trait:

| Site | Reached from |
|---|---|
| `liquers-core/src/value.rs` `Value::try_into_json_value` | a query link inside a parameter (`liquers-core/src/interpreter.rs`, the nested-link materialisation calling `value.try_into_json_value()`) |
| `liquers-lib/src/value/simple.rs` `SimpleValue::try_into_json_value` | `ExtValue` / `CombinedValue` delegation; a single-cell `RecordView` whose `Float` cell is NaN (`liquers-lib/src/value/mod.rs` `record_view_cell_as_base`); `liquers-lib/src/ui/commands.rs` |
| `liquers-py/src/value.rs` `Value::try_into_json_value` | the Python bindings — a panic across the PyO3 boundary becomes a `PanicException` |

An array or object containing one NaN element panics the same way, through the recursive arms.

**Should:** each call returns `Err` with `ErrorType::ConversionError`, a message naming the value
(`NaN`, `inf`, `-inf`) and saying that JSON has no representation for it. Arrays and objects
propagate the error through the existing `?`. Finite floats convert exactly as today.

### Scope and Acceptance Criteria

- **AC-1** NaN is refused, not a panic
  - WHEN `Value::F64(f64::NAN).try_into_json_value()` is called in `liquers-core`
  - THEN it returns `Err` whose `error_type` is `ErrorType::ConversionError` and whose message
    contains `NaN`
- **AC-2** Both infinities are refused
  - WHEN the same call is made with `f64::INFINITY` and with `f64::NEG_INFINITY`
  - THEN each returns a `ConversionError` naming `inf` / `-inf`
- **AC-3** A nested non-finite float fails the whole conversion
  - WHEN a `Value::Array` or `Value::Object` holding one NaN among finite values is converted
  - THEN the conversion returns the same `ConversionError`, and no partial JSON is produced
- **AC-4** Finite floats are unchanged
  - WHEN `Value::F64(1.5)`, `-0.0` and `f64::MAX` are converted
  - THEN the results equal `serde_json::json!(1.5)` etc., as before the fix
- **AC-5** `SimpleValue` (and so `ExtValue`) behaves the same
  - WHEN `SimpleValue::F64 { value: f64::NAN }` is converted in `liquers-lib`, directly and through
    `ExtValue`/`CombinedValue`
  - THEN it returns the AC-1 error
- **AC-6** A NaN record cell is refused, not a panic
  - WHEN a single-cell `RecordView` whose `Float` cell is NaN is converted with
    `try_into_json_value` (feature `records`)
  - THEN it returns a `ConversionError`
- **AC-7** The Python binding's `Value` behaves the same
  - WHEN `liquers-py`'s `Value::F64 { value: f64::NAN }` is converted
  - THEN it returns the AC-1 error (raised in Python as the mapped Liquers error, not a
    `PanicException`)

**Non-goals.** Mapping non-finite floats to `null` (rejected: it silently turns a float into
`None`, and diverges from `liquers-records`' `scalar_to_json`, which already errors). Changing
`Value::from_f64` to reject non-finite values (a NaN is a legitimate value; only its JSON form is
missing). `CommandParameterValue::from_float` in `liquers-core/src/command_metadata.rs` carries the
same `unwrap`, but it is private and has no caller; it is left alone (Q2).

### Systems touched and crate placement

- `liquers-core/src/value.rs` — the trait implementation for `Value`, plus one small public helper
  that turns an `f64` into a `serde_json::Number` or the AC-1 error, so the message is written once.
- `liquers-lib/src/value/simple.rs` — `SimpleValue` calls the core helper.
- `liquers-py/src/value.rs` — the Python `Value` calls the core helper.

All three crates already depend on `liquers-core`; the dependency flow in `CLAUDE.md` is respected.
No command, query syntax, registry entry or feature flag changes. `liquers-web` is unaffected: its
foreign value refuses JSON conversion outright and has no `F64` arm.

### Documentation intent

- **Reference:** `specs/reference/VALUE_TYPE_SYSTEM.md` — one sentence where it describes
  `try_into_json_value`: a non-finite float is a `ConversionError`. History row and `reviewed:` bump.
- **Guide:** none new. `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` mentions `try_into_json_value`;
  review it for a statement that conversion is infallible for scalars and correct it if present.
- **Other:** the trait doc comment on `ValueInterface::try_into_json_value` states the rule.
- **Issue:** close `VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT` with a resolution note (§4.3).

### Design Dependencies

- **overlaps** `core-value-enum-oversized` (weak: same file and trait method, different arms — that
  design moves the large variants behind `Arc` and leaves the scalar arms alone; whichever lands
  second rebases trivially).
- Precedent, not a dependency: `liquers-records/src/formats/ndjson.rs` `scalar_to_json` already
  refuses non-finite `Float` and `Vector` elements with an error.

Overlap triage: `grep` of `specs/index.csv` for `json`, `float`, `nan`, `core/value`, and of every
design folder for `try_into_json_value` / `from_f64`. No open design strongly overlaps (T1-T4 fail
for every candidate); the only weak overlap is listed above.

### Design Readiness

**Readiness:** `needs-decision` (until Q1 is answered).
**Automatic fixing:** `not-eligible` — the fix spans three crates (rule 6), adds one `pub` helper to
`liquers-core` (rule 4), and is `P0`, which needs the user's authorisation in any case.

Open questions, tiered (`autonomous_bulk_design.md` §3):

1. **Q1 — Open design: should the JSON *byte* path be in scope too?** *(needs decision)*
   `DefaultValueSerializer::as_bytes(.., "json")` for `Value` (`liquers-core/src/value.rs`) and its
   `SimpleValue` mirror serialise through serde, and `serde_json` writes NaN/∞ as `null` without
   an error. `json` is the default format of `F64`, so a cached NaN asset is written as `null` and
   reads back as `None` — a silent type change on a cache round trip, and the same contract
   ("what does a non-finite float become in JSON") answered the opposite way.
   **Recommendation: include it** as AC-8 (the write returns a `SerializationError`; the asset layer
   already tolerates a refused write by keeping the value in memory with metadata-only persistence),
   keeping the complexity at `S`/`M`. The alternative is to ship the P0 panic fix alone now and
   file the byte path as its own issue with `overlaps` to this design.
2. **Q2 — Proposed resolution:** leave the dead private `CommandParameterValue::from_float` as is
   (no caller; its inputs are compile-time literals, and the macro already guards them with
   `unwrap_or`). Removing it is a separate tidy-up, not this fix.
3. **Q3 — Implementation detail:** helper name and placement — proposed
   `pub fn json_number_from_f64(n: f64) -> Result<serde_json::Number, Error>` in
   `liquers-core/src/value.rs`, decided in Phase 2.

## Phase 2: Architecture

## Phase 3: Examples and Tests

## Phase 4: Implementation Plan
