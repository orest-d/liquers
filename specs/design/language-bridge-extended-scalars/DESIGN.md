---
id: LANGUAGE-BRIDGE-EXTENDED-SCALARS
kind: design
title: Python and JavaScript bridges map the extended scalars to native types
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [py, web, lib/value]
issues: [EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES]
created: 2026-10-27
---
# Python and JavaScript bridges map the extended scalars to native types

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written, not
approved, nothing implemented. Written against the unimplemented `extended-scalar-value-types`.

## Phase 1: High-Level Design

### Purpose

`extended-scalar-value-types` adds fifteen `ExtValue` scalars (`U64`, `I128`, `Decimal`, `Date`,
`DateTime`, `Duration`, `Uuid`, …), usable from Rust only: `liquers-py` refuses them and
`liquers-web` sends their JSON text. This design gives each bridge a two-way mapping table, so a
Python `date` or a JavaScript `bigint` arrives typed and leaves as the native value.

### Problem Example

Verified at HEAD (`04667a80`):

- `liquers-py/src/value.rs`: `py_to_simple` raises `ConversionError` for `2**63`; a
  `datetime.date` becomes `py.Object` (permissive) or is refused (strict); `ext_to_py` refuses every
  variant but `Foreign`, `PolarsDataFrame` and `Image`.
- `liquers-web/src/bridge.rs`: `bigint_to_value` refuses a `BigInt` outside `i64`, and
  `js_to_value_at` refuses a `Date` (test `value05_unknown_object_uses_opaque_value`).
  `default_value.rs` `to_js_custom` has no scalar arms.

Scenario, once the scalars exist: a Python command returns `datetime.date(2026, 10, 26)`. Today it
is a `py.Object`, so `emit-0/to_text` raises `ConversionError`. It should be a `Date`: `to_text`
gives `2026-10-26`, and Python gets a `date` back. A Rust command
returning `u64::MAX` should reach JavaScript as `18446744073709551615n`, not as the string
`"18446744073709551615"`.

### Scope and Acceptance Criteria

- **AC-1** Python values arrive typed
  WHEN a Python command returns `date`, `time` (naive), `datetime` (aware), `timedelta`, `Decimal`
  (finite, ≤ 28 digits), `UUID`, or an `int` in `[2**63, 2**128)` or `[-2**127, -2**63)`
  THEN the value has identifier `Date`, `Time`, `DateTime` (converted to UTC), `Duration`,
  `Decimal`, `Uuid`, `U64` / `I128` / `U128` respectively, in both conversion modes
- **AC-2** Python receives native values
  WHEN any of the fifteen scalars reaches Python
  THEN it is an `int`, `float`, `decimal.Decimal`, `datetime.date` / `time` / `datetime` (aware,
  UTC) / `timedelta` or `uuid.UUID`; a `DateTime` or `Duration` keeps microseconds and drops the
  rest
- **AC-3** Python compatibility
  WHEN an object of these types cannot be represented (naive `datetime`, aware `time`, `Decimal`
  NaN or with more than 28 digits, an `int` outside 128 bits, a container holding any of them)
  THEN it behaves exactly as today: opaque or refused per the mode, and an `int` raises
  `ConversionError`
- **AC-4** JavaScript values arrive typed
  WHEN a command returns a `Date` or a `BigInt` outside `i64`
  THEN the value is a `DateTime` (millisecond precision) or `U64` / `I128` / `U128`; an invalid
  `Date` and a `BigInt` outside 128 bits are a `conversion_error`
- **AC-5** JavaScript receives native values
  WHEN a scalar reaches JavaScript
  THEN `U64`, `I128`, `U128` are `bigint`; `I8`…`U32` and `F32` are `number`; `DateTime` is a
  `Date` (milliseconds kept); `Decimal`, `Date`, `Time`, `Duration`, `Uuid` are their canonical
  strings
- **AC-6** Rust commands read bridged values
  WHEN a value from AC-1 or AC-4 is the input of a Rust command (`to_text`)
  THEN it reads as the scalar's text form, as `extended-scalar-value-types` AC-4 defines

Non-goals: scalars nested in a container (it stays opaque or refused, as today); command
*arguments* of these types; JavaScript `Temporal`; `numpy` scalars; the core design's JSON forms.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It changes what both bindings return for existing inputs
  (rule 4), spans two crates (rule 6), and readiness is `needs-decision` (rule 5).
- **Leading issue:** **Open design - JavaScript forms of `DateTime` and the wide integers.**
- **Explanation:** each bridge has one place per direction where typed values are recognised
  (`py_to_value` / `ext_to_py`; `from_js_custom` / `to_js_custom`). Phases 3-4 hold under the
  recommended answers and the core design's assumptions.
- **Open questions:**
  1. **Open design - JavaScript forms.** Recommended: `U64` / `I128` / `U128` always leave as
     `bigint`, and `DateTime` as a `Date`. The alternative, the core design's JSON string, is
     lossless but makes pages parse every value; a size-dependent form is rejected as in JSON.
  2. **Open design - naive Python `datetime`.** Recommended: not mapped (AC-3), because a naive
     value has no instant and guessing UTC or local time changes data silently. Today it is a
     `py.Object`, and Python-to-Python pipelines keep working.
  3. **Proposed resolution - precision.** Python truncates `DateTime` / `Duration` to
     microseconds and JavaScript `DateTime` to milliseconds, documented, rather than refusing a
     value with nanoseconds (which `Utc::now()` gives).
  4. **Proposed resolution - feature.** `liquers-py` and `liquers-web` enable
     `liquers-lib/ext-scalars` unconditionally, so their matches need no `cfg`.
  5. **Assumption - core design.** Its decisions 1-4 are taken as recommended (one
     `ext-scalars` feature, the identifiers above, `rust_decimal`, `chrono::TimeDelta`).

### Design Dependencies

- `extended-scalar-value-types`: **requires**. It declares the variants, `From<T>` and
  `TryFrom<&ExtValue>`. It is `needs-decision`; its decisions are carried as assumption 5.
- `record-decimal-field-type`, `record-uuid-time-duration-types` (in review): **overlaps**. Their
  single-cell JavaScript forms (`single_cell_to_js`) are strings, as `Decimal` and `Uuid` are here.
- `PY-RECORD-VALUES-CANNOT-CROSS-TO-PYTHON` (designed in this run): **overlaps**. Both add arms to
  `ext_to_py`; T3 rejected, because the arms are independent and force no order.
- `error-type-taxonomy` (in review): **overlaps**; it may re-type bridge refusals.

### Consolidated Findings

- `extended-scalar-value-types` plans a `cfg`-gated arm in `liquers-web/src/default_value.rs`,
  but `liquers-web` has no `ext-scalars` feature to gate on (`liquers-lib` with
  `default-features = false`). Enabling it in the dependency line (Step 1) keeps the arms safe
  under feature unification; the core design's implementer should take that step.
- The JSON fallback already gives JavaScript the strings for `Decimal`, `Date`, `Time`,
  `Duration`, `Uuid`; only the wide integers and `DateTime` need arms.
- `JAVASCRIPT_GUIDE.md` §7 and test `value05` use a `Date` as *the* unconvertible object; both
  move to a `Map`. The guide's `liquers.opaque(today)` example holds: `from_js_custom` sees the
  wrapper, not the `Date`.
- Order: core design, then this one; both bindings in one PR, as the guides pair them.

## Phase 2: Architecture

### Solution

**Python** (`liquers-py/src/value.rs`). New private `py_to_scalar(object) -> Result<Option<ExtValue>,
Error>`, called by `py_to_value` before `py_to_simple`, at the top level only. In order:
`PyDateTime` (aware → `DateTime<Utc>`; naive → `None`), `PyDate`, `PyTime` (naive only),
`PyDelta`, an `int` that does not fit `i64` (`U64`, then `I128`, then `U128`; beyond → the existing
`ConversionError`), and `decimal.Decimal` / `uuid.UUID` recognised by qualified type name, as
polars is, and parsed from `str(object)`. `datetime` is tested before `date`, its superclass. New
private `scalar_to_py(py, &ExtValue) -> Result<Option<Py<PyAny>>, Error>`, tried first in
`ext_to_py`, as `if let` tests (the function's existing rule): the integers to `int`, `F32` to
`float`, temporal values through PyO3's `chrono` conversions (UTC `tzinfo`), `Decimal` /
`Uuid` by calling `decimal.Decimal(text)` / `uuid.UUID(text)`.

**JavaScript** (`liquers-web/src/default_value.rs`, `impl JsExtensionBridge for ExtValue`).
`from_js_custom` claims a `js_sys::Date` (`get_time()`; NaN → `ConversionError` "Invalid Date") and
a `BigInt` that does not fit `i64` (`U64` / `I128` / `U128`, beyond → `ConversionError` "outside the
128-bit range"); a `BigInt` inside `i64` returns `Ok(None)`, so the generic path keeps producing
`I64`. `to_js_custom` gains arms: wide integers → `js_sys::BigInt` from the decimal text;
`DateTime` → `js_sys::Date` from `timestamp_millis()` (outside ±8.64e15 ms → `ConversionError`);
other scalars keep `Ok(None)`, the JSON form. `bridge.rs` is unchanged (it names no value type).
No known issue blocks; the core design lands first.

Rejected: sniffing ISO strings into dates; nested scalars (base `Array` / `Object` cannot hold an
`ExtValue`); PyO3's `rust_decimal` / `uuid` features (the shared canonical text form needs none).

### Changes

- `liquers-py/Cargo.toml`: `liquers-lib` features gain `ext-scalars`; `pyo3` gains
  `features = ["chrono"]`; `rust_decimal`, `uuid` (already in the graph through `liquers-lib`).
- `liquers-py/src/value.rs`: `py_to_scalar`, `scalar_to_py`; `py_to_value`, `ext_to_py` call them;
  unit test `bool_is_not_an_int_and_big_ints_are_refused` becomes
  `bool_is_not_an_int_and_wide_ints_map` (AC-1, AC-3).
- `liquers-web/Cargo.toml`: `liquers-lib` features `["webui", "ext-scalars"]`.
- `liquers-web/src/default_value.rs`: the arms above replace the core design's `Ok(None)` for the
  wide integers and `DateTime`; no `_ =>` arm.
- Errors: `Error::conversion_error_with_message(source, target, reason)` throughout; no `unwrap`.
- Sync only. No command, no `specs/command_registry.yaml` change.
- Documents (History row, `reviewed:` bump): `reference/PYTHON_INTEGRATION.md` §5;
  `guides/PYTHON_GUIDE.md` §7; `guides/JAVASCRIPT_GUIDE.md` §7 (`Map` as the unconvertible
  example); `guides/LANGUAGE-INTEGRATION_GUIDE.md` §VALUE and its `DATANODE` remark "refuses
  `2**63` today"; both READMEs' `VALUE` rows. Proposed `affects_docs`: the four under `specs/`.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Code relied on `Date` being refused or `date` being `py.Object` | Values become typed, never lost; in both guides' History | medium |
| Precision loss (µs / ms) surprises a caller | Stated in the tables; tests pin it | high |
| `bigint` breaks `JSON.stringify` | Documented; only the wide integers use it | medium |
| Feature unification drops an arm | Feature always on (question 4) | high |
| wasm size grows (`rust_decimal`, `uuid`) | Small, pure Rust; the matrix's wasm32 row builds it | high |

Recovery: revert both crates. Tests that change: `value03_integer_boundaries`, `value05_…`,
`test_VALUE03_integer_beyond_i64_is_refused`, and the unit test above.

## Phase 3: Examples and Tests

### Examples

```python
@env.first_command(volatile=True)
def emit(n: int = 0): return PAYLOADS[n]
PAYLOADS[:] = [datetime.date(2026, 10, 26)]
env.evaluate("emit-0/to_text")      # "2026-10-26"           (AC-1, AC-6)
PAYLOADS[:] = [2**64 - 1]
env.evaluate("emit-0/same_obj")     # 18446744073709551615, an int (AC-1, AC-2)
```

```js
liquers.registerCommand({ name: "big", run: () => 2n ** 64n - 1n });
await liquers.evaluate("big");      // 18446744073709551615n (AC-4, AC-5)
```

Queries checked with `liquers-validate --command emit --command same_obj --command big`.

### Tests

Python, `liquers-py/tests/test_VALUE_value_bridge.py` (`bash scripts/test-liquers-py.sh`):

- `test_VALUE03_integer_beyond_i64_maps_to_wide_integers` (replaces `…_is_refused`):
  `2**63`, `2**64 - 1`, `-(2**127)`, `2**128 - 1` round-trip as `int` (AC-1, AC-2)
- `test_VALUE03_integer_beyond_128_bits_is_refused`: `2**128` raises `ConversionError` (AC-3)
- `test_VALUE08_temporal_values_round_trip`: `date`, naive `time`, aware `datetime` (a non-UTC one
  returns as the same instant in UTC), `timedelta` (AC-1, AC-2)
- `test_VALUE08_decimal_and_uuid_round_trip`: `Decimal("12.30")` keeps its scale (AC-1, AC-2)
- `test_VALUE08_nanoseconds_are_truncated`: a Rust `DateTime` with ns reaches Python with µs
  only (AC-2)
- `test_VALUE08_unmappable_values_behave_as_before`: naive `datetime`, `Decimal("NaN")`, a list
  holding a `date`: `py.Object` in permissive mode, `ConversionError` in strict (AC-3)
- `test_VALUE08_bridged_scalar_reads_in_rust_commands`: `emit-0/to_text` on a `date` (AC-6)

Rust unit, `liquers-py/src/value.rs`: `bool_is_not_an_int_and_wide_ints_map` (AC-1, AC-3).

JavaScript, `liquers-web/tests/value_bridge_VALUE.rs` (`cargo test -p liquers-web --target
wasm32-unknown-unknown --features debug-handles`):

- `value03_integer_boundaries` (changed): its 26-digit `BigInt` now maps to `I128`; a 40-digit
  one is the refused case (AC-4)
- `value03_wide_bigint_maps_to_wide_integers`: `2^63`, `2^64 − 1`, `−2^127` (AC-4)
- `value03_wide_integers_leave_as_bigint`: `U64`, `I128`, `U128` → equal `bigint` (AC-5)
- `value08_date_maps_to_datetime`: a `Date` → `DateTime`, back → a `Date` with the same
  `getTime()`; an invalid `Date` is refused (AC-4, AC-5)
- `value08_extended_scalars_reach_javascript`: `U8` → `number`, `Decimal("12.30")` → `"12.30"`,
  `Uuid` → its hyphenated string (AC-5)
- `value08_bridged_scalar_reads_in_rust_commands`: a `Date` command, then `to_text` (AC-6)
- `value05_unknown_object_uses_opaque_value`: uses a `Map` instead of a `Date` (regression)

## Phase 4: Implementation Plan

### Steps

Prerequisite: `extended-scalar-value-types` implemented.

- [ ] 1. `liquers-py/Cargo.toml`, `liquers-web/Cargo.toml` — features and dependencies of Phase 2 — `cargo check -p liquers-py` and `cargo check -p liquers-web --target wasm32-unknown-unknown`
- [ ] 2. `liquers-py/src/value.rs` `py_to_scalar`, `py_to_value` — inbound; the two Rust unit tests — `cargo test -p liquers-py --no-default-features --lib value`
- [ ] 3. `liquers-py/src/value.rs` `scalar_to_py`, `ext_to_py` — outbound; the Python tests — `bash scripts/test-liquers-py.sh`
- [ ] 4. `liquers-web/src/default_value.rs` `from_js_custom`, `to_js_custom` — the JavaScript tests, `value05` moved to `Map` — `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (and with `,records`)
- [ ] 5. Documents of Phase 2 with History rows; `JAVASCRIPT_GUIDE.md` §7 example — `./liquers-web/scripts/test-guide-examples.sh` and `python3 scripts/docs_index.py --check`
- [ ] 6. `specs/issues/EXTENDED-SCALARS-NOT-MAPPED-BY-LANGUAGE-BRIDGES.md` closed with a resolution note; this design `complete` — `python3 scripts/docs_index.py` then `--check`

Rollback: each step reverts alone.

### Validation

`bash scripts/check-build-matrix.sh` (a dependency's features change); `cargo fmt --check -p
liquers-py -p liquers-web`; both bindings' full loops as in `CLAUDE.md`. Diff review: two crates,
the documents above, no `specs/command_registry.yaml` change.
