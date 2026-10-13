---
id: VALUE-TO-JSON-NON-FINITE-FLOAT
kind: design
title: Non-finite floats in JSON — no panic, and one documented encoding
form: compact
workflow: liquers-project
status: in_review
phase: architecture
area: [core/value, lib/value, py]
issues: [VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT]
affects_docs: [specs/reference/VALUE_TYPE_SYSTEM.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, specs/guides/TYPE_SYSTEM_GUIDE.md]
created: 2026-10-13
---
# Non-finite floats in JSON — no panic, and one documented encoding

## Phase 1: High-Level Design

### Purpose

JSON cannot represent NaN, +∞ or −∞, and Liquers handles them inconsistently today: it panics, it
silently turns them into `None`, or it writes a file it cannot read back. This design sets one rule.
The in-memory conversion (`try_into_json_value`) returns an error. Writing JSON bytes is lossless
where the value is known to be a float, and writes `null` where it is not.

### Problem Example

```rust
let v = Value::from_f64(f64::NAN);
v.try_into_json_value();                                   // today: panics (from_f64(..).unwrap())
let b = v.as_bytes("json")?;                               // today: b"null"
Value::deserialize_from_bytes(&b, "F64", "json")?;         // today: Value::None
```

**The panic** is the `Number::from_f64(..).unwrap()` in three places:
- `Value::try_into_json_value` (`liquers-core/src/value.rs`), reached from parameter links
  (`interpreter.rs`).
- `SimpleValue::try_into_json_value` (`liquers-lib/src/value/simple.rs`), reached through
  `ExtValue`, a single-cell `RecordView`, and `ui/commands.rs`.
- The Python binding's `Value::try_into_json_value` (`liquers-py/src/value.rs`).

**The byte path** was measured on 2026-10-13 against `liquers-lib` and `serde_json` 1.0.154.
serde_json always writes `null` for a non-finite float, a custom `Formatter` only ever receives
`write_null()`, and reading `null`, `NaN` or `"NaN"` into an `f64` fails.

| Value | `as_bytes("json")` | Read back with the same identifier |
|---|---|---|
| core `F64(NaN)` | `null` | `None` |
| core `Array[F64(1.0), F64(NaN)]` (untagged) | `[1.0,null]` | `[F64(1.0), None]` |
| `SimpleValue::F64(NaN)` | `null` | `None` |
| `SimpleValue::Array[…, F64(NaN)]` (tagged) | `{"Array":{"value":[…,{"F64":{"value":null}}]}}` | **an unrelated `Object`**: the tagged read fails, and `from_plain_json` returns the tag structure as data |

`txt` round-trips a scalar. `SimpleValue` cannot write `yaml`, and its `yaml` reader turns `.nan`
into `None`. Today no format keeps a non-finite float inside a container.

### The non-finite float policy

Decided by the maintainer on 2026-10-13 (Q1).

| Where the float is | JSON written | JSON read |
|---|---|---|
| `try_into_json_value`, at any depth | — (a `ConversionError`) | — |
| Top-level `F64`, core and `SimpleValue` | `"NaN"`, `"Infinity"`, `"-Infinity"` | identifier `F64`: a number, a token, or `null` → NaN |
| Inside a `SimpleValue` container (tagged) | the same tokens | a number, a token, or `null` → NaN |
| Inside a core `Array` / `Object` (untagged) | `null` | `None`: **lossy by design** |

The tokens follow the protobuf JSON mapping, and reading `null` as NaN also accepts output from
JavaScript and pandas.

**Why nested floats become `null`.** A failed write stores nothing (the asset falls back to
metadata-only storage), which is worse for a user than a few `null`s inside an expensive result.
This is a compromise. It makes JSON look safer than it is, and the loss is silent
(`SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`). When fidelity matters, JSON is the wrong format, but
a lossless one for core containers does not exist yet (`DATA-FORMAT-CONSTANTS-AND-TOOLING`).

**Why the in-memory conversion still errors (Q2).** `serde_json::Value` cannot hold the float. A
token would bind as text, and `null` would bind a parameter as `None`. A refused conversion loses
nothing.

### Scope and Acceptance Criteria

- **AC-1** NaN is refused, not a panic
  - WHEN `Value::F64(f64::NAN).try_into_json_value()` is called
  - THEN it returns a `ConversionError` whose message contains `NaN`
- **AC-2** Both infinities are refused
  - WHEN the same is done with `f64::INFINITY` and `f64::NEG_INFINITY`
  - THEN each returns a `ConversionError` naming `inf` / `-inf`
- **AC-3** A nested non-finite float fails the whole in-memory conversion
  - WHEN a `Value::Array` or `Value::Object` holding a NaN is converted with `try_into_json_value`
  - THEN it returns the AC-1 error
- **AC-4** Finite floats are unchanged
  - WHEN `F64(1.5)`, `-0.0` and `f64::MAX` are converted and written as JSON
  - THEN the JSON value and bytes equal those from before the fix
- **AC-5** `SimpleValue` and `ExtValue` refuse the same way
  - WHEN `SimpleValue::F64(NaN)` is converted, directly and through `ExtValue` / `CombinedValue`
  - THEN it returns the AC-1 error
- **AC-6** A NaN record cell is refused
  - WHEN a single-cell `RecordView` with a NaN `Float` is converted (feature `records`)
  - THEN it returns a `ConversionError`
- **AC-7** The Python binding refuses the same way
  - WHEN `liquers-py`'s `Value::F64(NaN)` is converted
  - THEN it returns the AC-1 error
- **AC-8** A top-level core float round-trips
  - WHEN `Value::F64` holding NaN, +∞ or −∞ is written with `as_bytes("json")` and read as `F64`
  - THEN the bytes are `"NaN"` / `"Infinity"` / `"-Infinity"` and the value read back equals the
    original (NaN `is_nan()`)
- **AC-9** The `F64` reader accepts every encoding
  - WHEN `1.5`, `1`, `"NaN"`, `"Infinity"`, `"-Infinity"` or `null` is read as `F64` from `json`
  - THEN the results are 1.5, 1.0, NaN, +∞, −∞ and NaN. Any other string is a `SerializationError`,
    and `"NaN"` read as `Text` stays `Text`
- **AC-10** A nested core float is written as `null`
  - WHEN a core `Array` / `Object` holding NaN is written with `as_bytes("json")`
  - THEN the write succeeds with `null` in that position, which reads back as `None`
- **AC-11** `SimpleValue` floats round-trip at any depth
  - WHEN `SimpleValue::F64` with NaN / ±∞, alone and inside a `SimpleValue::Array`, is written and
    read back
  - THEN the same structure returns. The earlier `{"F64":{"value":null}}` form reads as the
    `Array` with NaN, not as an `Object`

**Non-goals:** rejecting NaN in `Value::from_f64`; reading Python's bare `NaN` literals (not
JSON); the lossy-write warning and a lossless format (both filed); `liquers-py`'s JSON byte path
(`PY-VALUE-SERIALIZER-IS-A-STUB`).

### Systems touched and crate placement

The rule lives in `liquers-core/src/value.rs`. `liquers-lib/src/value/simple.rs` and
`liquers-py/src/value.rs` call it, and both already depend on core. `liquers-core/src/command_metadata.rs`
loses the unused `CommandParameterValue::from_float` (Q3). No command, query syntax, registry entry or
feature changes. `liquers-web` is unaffected: its value refuses JSON outright.

### Documentation intent

- **Guide:** a new `specs/guides/SERIALIZATION_GUIDE.md`, written in Phase 5 (guides must be true at
  HEAD). It holds the policy and its reasoning, a float-fidelity table per format, and "don't use
  JSON when fidelity matters".
- **Reference:** a paragraph in `VALUE_TYPE_SYSTEM.md`.
- **Other:** doc comments. One-line pointers in `LANGUAGE-INTEGRATION_GUIDE.md` and
  `TYPE_SYSTEM_GUIDE.md`.
- **Issues:** close the source issue.

### Scope Changes

2026-10-13: extended from the in-memory panic (`S`) to the JSON byte path. Both answer what a
non-finite float becomes in JSON (overlap test T2). The size is now `M`.

### Design Dependencies

- **overlaps** `core-value-enum-oversized`: weak, the same `Value` match arms for other variants.
- **overlaps** `PY-VALUE-SERIALIZER-IS-A-STUB`: a future Python JSON path should follow this policy.
- **required-by** `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`, whose preferred home is the
  serialization redesign in `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`.
- **overlaps** `DATA-FORMAT-CONSTANTS-AND-TOOLING`, updated with this case (CBOR and YAML as lossless
  formats).
- Precedent: `liquers-records` `scalar_to_json` refuses non-finite floats. It writes typed table
  columns, and it stays as it is.

Overlap triage: `specs/index.csv` searched for `json`, `float`, `nan`, `serializ`, `cbor`,
`core/value`, and the design folders for `try_into_json_value` / `from_f64`. No strong overlap.

### Open Questions

Readiness is assessed after Phase 4. Automatic fixing is expected to be `not-eligible`: three
crates, a new public module, a changed serialized format, and `P0`.

1. **Q1, decided 2026-10-13:** the policy above, with nested untagged floats written as `null`.
2. **Q2, decided 2026-10-13:** `try_into_json_value` refuses at every depth.
3. **Q3, decided 2026-10-13:** delete `CommandParameterValue::from_float`: private, no caller, same
   `unwrap`.
4. **Q4, resolved in Phase 2:** `SimpleValue` uses a field-level serde attribute.

## Phase 2: Architecture

### Solution

A new public module, `liquers_core::value::json_f64`, owns the rule: the tokens, the strict
conversion, the lossless pair, and a serde `with` pair. Every site calls it, so no crate keeps its
own copy.

- **Core `Value` is untagged, so it gets explicit arms and no serde attribute.** `serialize_with` on
  `F64` would also turn nested floats into tokens, and those read back as `Text("NaN")`. So the
  `json` arm of `as_bytes` writes the top-level `F64` as a token, and nested floats keep serde's
  `null` (AC-10; that code does not change). A new `"F64"` arm in `deserialize_from_bytes`, next
  to the existing `"Bytes"` one, reads every encoding.
- **`SimpleValue` is externally tagged, so it gets
  `#[serde(with = "liquers_core::value::json_f64")]` on `F64 { value }`.** The tag rules out any
  confusion with `Text`. The attribute fixes the nested write, and its lenient `deserialize` lets
  the tagged read succeed instead of falling through to `from_plain_json` (AC-11). Nothing else
  uses `SimpleValue`'s derived serde form, so the attribute affects JSON only (Q4). The top-level
  arm and a new `"F64"` read arm call the same helpers as core.
- **`try_into_json_value` (core, `SimpleValue`, Python)** calls `json_f64::to_number(*n)?`. The
  recursive arms already propagate with `?`. `ExtValue` and `CombinedValue` delegate to
  `SimpleValue`. A single-cell `RecordView` goes through `record_view_cell_as_base`. A multi-cell
  view already errors in `liquers_records::to_json`.

**Rejected:**
- A custom `Formatter`: it never sees the value.
- `serialize_with` on core `F64`: nested tokens would read back as `Text`.
- Tagging core `Value`'s serde form: it would change every stored file.
- `null` from `try_into_json_value` (Q2).
- A new `ErrorType`: the existing ones fit.

**Known-issue preflight:** nothing blocks.
- `CORE-VALUE-ENUM-OVERSIZED` edits the other `Value` arms. Whichever lands second rebases.
- `simple-value-serializer-parity` is already implemented, and this design builds on its arms.
- `PY-VALUE-SERIALIZER-IS-A-STUB` limits the Python change to `try_into_json_value`.

**Command namespaces:** none. The registry and every `version: auto` body are untouched.

### Changes

New in `liquers-core/src/value.rs`. Everything is sync, pure and CPU-only, and wasm32-safe:

```rust
/// How Liquers writes an `f64` to JSON. See `specs/guides/SERIALIZATION_GUIDE.md`.
pub mod json_f64 {
    pub const NAN: &str = "NaN";
    pub const INFINITY: &str = "Infinity";
    pub const NEG_INFINITY: &str = "-Infinity";
    /// Finite → JSON number; NaN/±∞ → `Error::conversion_error_with_message(n, "JSON number", …)`.
    pub fn to_number(n: f64) -> Result<serde_json::Number, Error>;
    /// Lossless: a number when finite, else the token string.
    pub fn to_json(n: f64) -> serde_json::Value;
    /// A number, a token, or `null` (→ NaN); anything else is
    /// `Error::from_error(ErrorType::SerializationError, …)`.
    pub fn from_json(value: &serde_json::Value) -> Result<f64, Error>;
    /// serde `with` pair; `deserialize` uses a visitor (f64/i64/u64/str/unit/none).
    pub fn serialize<S: serde::Serializer>(n: &f64, serializer: S) -> Result<S::Ok, S::Error>;
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error>;
}
```

Changed:
- **Core `Value`:**
  - `try_into_json_value`, the `F64` arm, uses `to_number`.
  - The `json` arm of `as_bytes` writes `F64(x)` as `to_json(*x)`. A finite float gives the same
    `Number` as before (AC-4).
  - `deserialize_from_bytes` gets a `json`/`"F64"` arm: parse as `serde_json::Value`, then
    `from_json`.
  - Doc comments on both trait methods.
- **`SimpleValue`:**
  - The serde attribute on `F64`.
  - The same three changes as core: the `try_into_json_value` arm, the `as_bytes` `json`/`F64` arm,
    and the `"F64"` read arm.
- **Python `Value`:** the `F64` arm of `try_into_json_value`.
- **`command_metadata.rs`:** remove `from_float`.

**Errors and conventions:**
- The new errors are a `ConversionError` for the in-memory conversion and a `SerializationError`
  for reading.
- No `unwrap`, `expect` or `Error::new` is added.
- The `_` arms match on `&str` or on the external `serde_json::Value`.
- No feature or dependency is added.

**Documents (Phase 5):**
- `specs/guides/SERIALIZATION_GUIDE.md` (new, `area: [core/value, lib/value]`); the README `guides`
  block is generated.
- `VALUE_TYPE_SYSTEM.md`: a paragraph on `F64` in `json`, plus a History row.
- `LANGUAGE-INTEGRATION_GUIDE.md`: its NaN design question (line 353) points to the guide.
- `TYPE_SYSTEM_GUIDE.md`: one line saying to use `json_f64`.

### Risks

- **Reading old cache files:** a `null` under `F64` now reads as NaN instead of `None`, and JSON `1`
  reads as `F64(1.0)` instead of `I32(1)`. Both now follow the declared identifier, as intended.
  Phase 5 records this.
- **Writing:** a non-finite top-level `F64` is now `"NaN"`, which an older Liquers reads as `Text`.
  This is forward-only, and no mixed-version store is in use.
- **Tests likely to change:** the round-trip tests in `simple.rs` and `value.rs`. Run both crates
  with `records` on, and `cargo check -p liquers-py`.
- **Certainty: high.** The current behaviour was measured, and the serde mechanisms are standard.
- **Recovery:** each step is a revertible commit.

## Phase 3: Examples and Tests

## Phase 4: Implementation Plan
