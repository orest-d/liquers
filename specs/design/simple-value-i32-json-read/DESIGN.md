---
id: SIMPLE-VALUE-I32-JSON-READ
kind: design
title: liquers-lib's base value reads an I32 written as JSON back as I32
form: compact
status: complete
gh_pr: [210]
readiness: ready
autofix: eligible
area: [lib/value]
issues: [SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64]
created: 2026-10-26
---
# liquers-lib's base value reads an I32 written as JSON back as I32

> **Complete on 2026-10-27**: implemented by automatic fix in #210.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

A value read back from storage must report the identifier it was written with
(`VALUE_TYPE_SYSTEM.md`: the identifier is the dispatch key). `SimpleValue`, the base half of
`liquers_lib::value::Value`, breaks this for one scalar: an `I32` written as `json` comes back as
`I64`. Read it as `I32`.

### Problem Example

Verified at HEAD (`liquers-lib/src/value/simple.rs`, `impl DefaultValueSerializer for SimpleValue`,
`deserialize_from_bytes`, `"json"` arm):

```rust
let bytes = SimpleValue::I32 { value: 42 }.as_bytes("json")?;          // b"42"
SimpleValue::deserialize_from_bytes(&bytes, "I32", "json")?
// today:     SimpleValue::I64 { value: 42 }  — no "I32" arm; falls to from_plain_json, which
//            turns every JSON integer into I64
// should be: SimpleValue::I32 { value: 42 }
```

`json` is a declared format of `I32` (shared `TypeInfo` with core), so a keyed asset holding an
`I32` reloads through the fast track as an `I64`. Two tests pin the defect as intended:
`test_json_roundtrip_i32` expects `I64`, and `every_declared_format_round_trips` maps
`("json", I32)` to an `I64` expectation.

### Scope and Acceptance Criteria

- **AC-1** An I32 round-trips
  WHEN `SimpleValue::I32 { value: 42 }` and `I32 { value: -7 }` are written as `json` and read back
  with identifier `I32`
  THEN they read back equal, with identifier `I32`
- **AC-2** Every declared JSON pair round-trips exactly
  WHEN `every_declared_format_round_trips` runs over `SimpleValue::type_descriptions()`
  THEN no `json` pair needs a special expectation
- **AC-3** A payload that is not an i32 is refused
  WHEN `b"5000000000"`, `b"1.5"` or `b"\"x\""` is read as `json` with identifier `I32`
  THEN `deserialize_from_bytes` returns a `SerializationError` naming `I32`
- **AC-4** Everything else is unchanged
  WHEN JSON is read with an empty identifier, an identifier `SimpleValue` does not own, or any
  other base identifier (`I64`, `F64`, `Bool`, `Text`, `None`, the structured ones)
  THEN it is read exactly as at HEAD
- **AC-5** The combined value follows
  WHEN a `liquers_lib::value::Value` holding the base `I32 { value: 3 }` is written as `json` and
  read with `Value::deserialize_from_bytes(.., "I32", "json")`
  THEN it reads back with identifier `I32`

Non-goals: nested integers inside `Array`/`Object` (tagged JSON already keeps them); strict reads
for the shape-typed identifiers (`Bool`, `Text`, `None`, `I64`), which already round-trip; core
`Value` (`core-value-json-identifier-dispatch`).

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring the documented round-trip rule in one
  function of `liquers-lib/src/value/simple.rs`, plus two test expectations; no `pub` item, format,
  registry or command change.
- **Leading issue:** None.
- **Explanation:** the arm follows the pattern of the `"F64"` arm beside it and of the core fix.
- **Open questions:** None. Implementation detail settled: only `I32` gets a strict arm. `I64`,
  `Bool`, `Text` and `None` already round-trip through `from_plain_json`, and making them strict
  would change reads of hand-written files for no reported defect (the issue's "for symmetry" set is
  recorded as considered and not taken). AC-3's refusal matches the core design's AC-4 wording.

### Design Dependencies

- `core-value-json-identifier-dispatch` — **overlaps**, weak: the same defect class in core
  `Value` (`liquers-core/src/value.rs`); a different function and crate, no order. Its Phase 4 is
  finished, so it is not extended.
- `data-format-vocabulary` — **overlaps**, weak: may later replace the `"json"` literal; no order.

### Consolidated Findings

- `SimpleValue` already dispatches its structured identifiers in this arm; `I32` is the only
  declared scalar whose identifier is lost, because `try_from_json_value` maps every integer to
  `I64`.
- `CombinedValue` offers an identifier the extension does not declare to the base value first
  (`combined-value-identifier-dispatch`), so AC-5 needs no change outside `simple.rs`.

## Phase 2: Architecture

### Solution

`liquers-lib/src/value/simple.rs`, `deserialize_from_bytes`, `"json"` arm, a new arm before the
fallback:

```rust
"I32" => serde_json::from_slice::<i32>(b)
    .map(|value| SimpleValue::I32 { value })
    .map_err(|e| Error::from_error(
        ErrorType::SerializationError,
        format!("JSON error reading I32 in deserialize_from_bytes: {e}"),
    )),
```

`serde_json` refuses an out-of-range integer, a float and a string for `i32` (AC-3). The comment
above the arm names `I32` among the identifiers read as their own payload. Sync, no allocation.

Rejected: widening `try_from_json_value` (it is the shape reader for untyped JSON and nested
values; it has no identifier); reading as `i64` and narrowing (same result, two error paths).

Known-issue preflight: `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` decides which arm runs, not
what this arm does. Nothing open blocks.

### Changes

- `liquers-lib/src/value/simple.rs`: the `"I32"` arm and comment; `test_json_roundtrip_i32`
  expects `I32 { value: 42 }`; the `("json", SimpleValue::I32 { .. })` special case is removed
  from `every_declared_format_round_trips`; new tests below.
- No reference change: `VALUE_TYPE_SYSTEM.md` already states the rule this restores.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-lib/src/value/simple.rs` |
| Existing tests | the two named tests change expectation (intended); others unaffected |
| Compatibility / data | a stored `I32` written as JSON now reloads as `I32`; a stored file with identifier `I32` whose JSON is not an `i32` now fails to deserialize (the asset layer recomputes) instead of reading as another type |
| Recovery | revert the commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
let v = SimpleValue::I32 { value: -7 };
assert_eq!(SimpleValue::deserialize_from_bytes(&v.as_bytes("json")?, "I32", "json")?, v);
assert!(SimpleValue::deserialize_from_bytes(b"1.5", "I32", "json").is_err());
assert_eq!(SimpleValue::deserialize_from_bytes(b"5", "", "json")?, SimpleValue::I64 { value: 5 });
```

### Tests

Unit, `liquers-lib/src/value/simple.rs` `mod tests`:

- `test_json_roundtrip_i32` — expects `I32 { value: 42 }`; add `-7`: AC-1
- `every_declared_format_round_trips` — the `("json", I32)` special case removed: AC-2
- `json_i32_with_a_mismatched_payload_is_refused` — `5000000000`, `1.5`, `"x"`:
  `ErrorType::SerializationError`, message contains `I32`: AC-3
- `untyped_json_integer_still_reads_as_i64` — `b"5"` with `""` and with `"test.Foreign"`: both
  `I64 { value: 5 }`, as at HEAD (the fallback arm reads by shape): AC-4

Unit, `liquers-lib/src/value/extended.rs` `mod tests` (or `value/mod.rs`):

- `combined_value_reads_json_i32_as_i32` — `Value::from(SimpleValue::I32 { value: 3 })` through
  `as_bytes("json")` and `Value::deserialize_from_bytes(.., "I32", "json")`; identifier `I32`: AC-5

Run: `cargo test -p liquers-lib --lib value::simple value::extended`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-lib/src/value/simple.rs` `mod tests` — change the two expectations, add `json_i32_with_a_mismatched_payload_is_refused` and `untyped_json_integer_still_reads_as_i64`; the I32 ones fail at HEAD — `cargo test -p liquers-lib --lib value::simple`.
- [x] 2. `simple.rs` `deserialize_from_bytes` — the `"I32"` arm and comment — `cargo test -p liquers-lib --lib value::simple`. Rollback: revert.
- [x] 3. `combined_value_reads_json_i32_as_i32` — `cargo test -p liquers-lib --lib value::extended`.
- [x] 4. Close `SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `cargo test -p liquers-lib --no-default-features --lib --tests`
(the arm is not gated); diff review: only `simple.rs`, the test file touched and the issue.
