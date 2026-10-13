---
id: VALUE-TO-JSON-NON-FINITE-FLOAT
kind: design
title: Non-finite floats in JSON — no panic, and one documented encoding
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [core/value, lib/value, py]
issues: [VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT]
created: 2026-10-13
---
# Non-finite floats in JSON — no panic, and one documented encoding

## Phase 1: High-Level Design

### Purpose

JSON cannot represent NaN, +∞ or −∞, and today Liquers handles them three different ways: it panics,
it silently turns them into `None`, or it writes a file it cannot read back. This design defines one
rule for both JSON paths. Converting a value in memory (`try_into_json_value`) returns an error.
Writing JSON bytes (`as_bytes("json")`) is lossless wherever the value is known to be a float, and
writes `null` where it is not.

### Problem Example

```rust
use liquers_core::value::{Value, ValueInterface, DefaultValueSerializer};

let v = Value::from_f64(f64::NAN);           // accepted: from_f64 has no finiteness check
let json = v.try_into_json_value();          // today: panics, serde_json::Number::from_f64(..).unwrap()
let bytes = v.as_bytes("json")?;             // today: b"null"
let back = Value::deserialize_from_bytes(&bytes, "F64", "json")?;   // today: Value::None
```

**Today: the in-memory conversion panics.** The `unwrap` appears in three places:

| Site | Reached from |
|---|---|
| `liquers-core/src/value.rs` `Value::try_into_json_value` | a query link inside a parameter (`liquers-core/src/interpreter.rs`, nested-link materialisation) |
| `liquers-lib/src/value/simple.rs` `SimpleValue::try_into_json_value` | `ExtValue` / `CombinedValue` delegation; a single-cell `RecordView` with a NaN `Float` cell (`liquers-lib/src/value/mod.rs` `record_view_cell_as_base`); `liquers-lib/src/ui/commands.rs` |
| `liquers-py/src/value.rs` `Value::try_into_json_value` | the Python bindings; a panic across PyO3 becomes a `PanicException` |

**Today: the byte path gives wrong results without any error.** These results were measured on
2026-10-13 with a throwaway test against `liquers-lib` and `serde_json` 1.0.154. The serializer
writes `null` for every non-finite float and has no option to do otherwise. A custom `Formatter`
cannot intercept this either, because it receives `write_null()` and never sees the float. Reading
a `null` into an `f64`, the bare literal `NaN`, or the string `"NaN"` all fail.

| Value | `as_bytes("json")` | read back (`json`, same identifier) |
|---|---|---|
| core `Value::F64(NaN)` | `null` | `Value::None`: the value silently changes type |
| core `Value::Array([F64(1.0), F64(NaN)])` (untagged) | `[1.0,null]` | `[F64(1.0), None]` |
| `SimpleValue::F64 { value: NaN }` | `null` (scalars are written plain) | `None` |
| `SimpleValue::Array([F64(1.0), F64(NaN)])` (written tagged) | `{"Array":{"value":[{"F64":{"value":1.0}},{"F64":{"value":null}}]}}` | **an unrelated value.** The tagged read fails on the `null`, the fallback `from_plain_json` reads the tag structure as data, and the result is `Object {"Array": Object {"value": Array [Object {"F64": …}, …]}}` |

`txt` round-trips a scalar (`inf` → `F64(inf)`). `SimpleValue` cannot write `yaml` ("Unsupported
format yaml"), and its `yaml` reader goes through `serde_json::Value`, so `.nan` reads as `None`.
**Today no format keeps a non-finite float inside a container.**

**Should:**
- The in-memory conversion returns an error.
- A value whose float-ness is known survives a JSON round trip exactly.
- A float nested in a core array or object, where JSON keeps no type, becomes `null`. This is a
  deliberate, documented loss.

### The non-finite float policy

The policy is decided (Q1, maintainer, 2026-10-13). The guide planned below states it as the rule.

| Where the float is | JSON written | JSON read |
|---|---|---|
| `try_into_json_value`, at any depth | — (a `ConversionError`) | — |
| Top level of `as_bytes("json")`: core `Value::F64`, `SimpleValue::F64` | the string `"NaN"`, `"Infinity"` or `"-Infinity"` | with identifier `F64`: a number, one of the three strings, or `null` → `NaN` |
| Nested in a `SimpleValue` container (written tagged, so each element keeps its type) | the same strings in the `F64` `value` field | a number, one of the strings, or `null` → `NaN` |
| Nested in a core `Value::Array` / `Value::Object` (untagged) | `null` | `Value::None`: **lossy, by design** |

The string tokens follow the protobuf JSON mapping for `float` / `double`, which is the most widely
used lossless convention. A reader that does not know the convention still sees valid JSON. Reading
`null` as NaN also accepts files from JavaScript (`JSON.stringify` writes `null`) and from pandas
`to_json`. It cannot recover the sign of an infinity.

**Why `null` for nested floats rather than an error.** A user may store the result of an expensive
operation. If the write fails, nothing is written (the asset layer falls back to metadata-only
storage, `liquers-core/src/assets.rs`, the `Err(_) =>` arm after `state.as_bytes()`), and that is
more disruptive than a few `null`s inside a large result. This is a compromise, not the cleanest
answer. It makes JSON look safe for data where it is not, and the loss raises no warning, because
`as_bytes` has no access to the asset's metadata log (filed as
`SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`). When float fidelity matters, JSON is the wrong
format. After this design, a `liquers-lib` value written as JSON is lossless, because its containers
are tagged. A core `Value` container still has no lossless format. CBOR and pickle are declared as
media types but not implemented for base values, and YAML is read through `serde_json::Value` and
cannot be written at all. All three belong to the generic serde path of
`DATA-FORMAT-CONSTANTS-AND-TOOLING`, which has been updated with this case.

**Why the in-memory conversion still errors.** `serde_json::Value` cannot hold a non-finite float.
The token string would be bound as text, and `null` would bind a parameter link to `None`. A refused
conversion is cheap to recover from: nothing is lost, and the caller can map the value itself.
This is Q2, now decided.

### Scope and Acceptance Criteria

- **AC-1** NaN is refused, not a panic
  - WHEN `Value::F64(f64::NAN).try_into_json_value()` is called in `liquers-core`
  - THEN it returns `Err` with `ErrorType::ConversionError` and a message containing `NaN`
- **AC-2** Both infinities are refused
  - WHEN the same call is made with `f64::INFINITY` and with `f64::NEG_INFINITY`
  - THEN each returns a `ConversionError` naming `inf` / `-inf`
- **AC-3** A nested non-finite float fails the whole in-memory conversion
  - WHEN a `Value::Array` or `Value::Object` holding one NaN among finite values is converted with
    `try_into_json_value`
  - THEN it returns the same `ConversionError`
- **AC-4** Finite floats are unchanged on both paths
  - WHEN `Value::F64(1.5)`, `-0.0` and `f64::MAX` are converted and written as JSON
  - THEN the JSON value and the bytes are identical to what was produced before the fix
- **AC-5** `SimpleValue` and `ExtValue` refuse in memory the same way
  - WHEN `SimpleValue::F64 { value: f64::NAN }` is converted with `try_into_json_value`, directly and
    through `ExtValue` / `CombinedValue`
  - THEN it returns the AC-1 error
- **AC-6** A NaN record cell is refused, not a panic
  - WHEN a single-cell `RecordView` whose `Float` cell is NaN is converted (feature `records`)
  - THEN it returns a `ConversionError`
- **AC-7** The Python binding refuses in memory the same way
  - WHEN `liquers-py`'s `Value::F64 { value: f64::NAN }` is converted with `try_into_json_value`
  - THEN it returns the AC-1 error, not a panic
- **AC-8** A top-level core float round-trips through JSON bytes
  - WHEN `Value::F64` holding NaN, +∞ and −∞ is written with `as_bytes("json")` and read with
    identifier `F64`
  - THEN the bytes are `"NaN"`, `"Infinity"`, `"-Infinity"`, and reading gives back NaN, +∞, −∞
- **AC-9** The `F64` reader accepts every encoding
  - WHEN the bytes `1.5`, `"NaN"`, `"Infinity"`, `"-Infinity"` or `null` are read with identifier
    `F64` and format `json`
  - THEN they give `F64(1.5)`, NaN, +∞, −∞ and NaN. Any other string is a `SerializationError`.
    With identifier `Text`, the string `"NaN"` still reads as `Text("NaN")`
- **AC-10** A nested core float is written as `null`
  - WHEN a `Value::Array` / `Value::Object` holding NaN is written with `as_bytes("json")`
  - THEN the write succeeds with `null` at that position, and reading it back gives `Value::None`
    there. This is the documented loss.
- **AC-11** `SimpleValue` floats round-trip at any depth
  - WHEN `SimpleValue::F64` holding NaN, +∞ or −∞, alone and inside a `SimpleValue::Array`, is
    written with `as_bytes("json")` and read back
  - THEN the value read back is the same `SimpleValue` structure, with the infinities equal and NaN
    `is_nan()`. A file in the earlier `{"F64":{"value":null}}` form now reads as the `Array` with
    NaN, not as the unrelated `Object` it reads as today

**Non-goals.**
- Changing `Value::from_f64` to reject non-finite values: NaN is a legitimate value, only its JSON
  form is missing.
- Reading Python's bare `NaN` / `Infinity` literals, which are invalid JSON and would need a
  different parser.
- Emitting a warning for the lossy write (`SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`).
- Implementing a lossless format (`DATA-FORMAT-CONSTANTS-AND-TOOLING`).
- `CommandParameterValue::from_float` (Q3).
- `liquers-py`'s JSON byte path, which does not exist (`PY-VALUE-SERIALIZER-IS-A-STUB`).

### Systems touched and crate placement

- `liquers-core/src/value.rs`:
  - `Value::try_into_json_value`.
  - `impl DefaultValueSerializer for Value`: the `json` arms of `as_bytes` and
    `deserialize_from_bytes`, where an `"F64"` arm sits beside the existing `"Bytes"` one.
  - Small public helpers for the shared rules (`f64` → `serde_json::Number` or error; `f64` ↔ token),
    so `liquers-lib` and `liquers-py` do not copy them.
- `liquers-lib/src/value/simple.rs`: `SimpleValue::try_into_json_value`, and the `F64` `value`
  field's serde encoding, using the core helpers.
- `liquers-py/src/value.rs`: `Value::try_into_json_value` only.

Every crate involved already depends on `liquers-core`. Nothing changes in commands, query syntax,
the registry or feature flags. `liquers-web` is not affected: its value refuses JSON conversion
outright.

### Documentation intent

- **Guide (new):** `specs/guides/SERIALIZATION_GUIDE.md`, titled "Choosing a serialization format".
  It covers:
  - the non-finite float policy above, with its reasoning, including that `null` is a compromise
    with no warning;
  - a per-format fidelity table for floats (`json`, `txt`, `yaml`);
  - a plain recommendation: never rely on JSON when float fidelity matters, use a lossless format,
    and the binary formats are not available yet.

  It is written in Phase 5, because a guide must be true at HEAD. Any other serialization pitfall
  met while writing it goes in too.
- **Reference:** `specs/reference/VALUE_TYPE_SYSTEM.md`: one paragraph on how `F64` is written in
  `json` and how the `F64` identifier is read back, linking the guide. Add a History row and bump
  `reviewed:`.
- **Other:**
  - the doc comments on `ValueInterface::try_into_json_value` and `DefaultValueSerializer::as_bytes`
    state the rule;
  - review `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (it mentions `try_into_json_value`) for any
    claim that scalar conversion cannot fail;
  - link the new guide from `specs/README.md`.
- **Issues:** close `VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT` (§4.3). The follow-up issue
  `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE` stays open.

### Scope Changes

2026-10-13: extended from the in-memory panic (complexity `S`) to the JSON byte path, because both
answer the same question: what a non-finite float becomes in JSON (overlap test T2, shared
contract). The design is now `M` and still fits the compact form.

### Design Dependencies

- **overlaps** `core-value-enum-oversized`. This is weak overlap: the same file and the same trait
  methods, but different arms. That design moves the large variants behind `Arc`. Whichever lands
  second rebases.
- **overlaps** `PY-VALUE-SERIALIZER-IS-A-STUB`. Once Python gets a JSON byte path, it should follow
  this policy.
- **required-by** `SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`, filed with this phase. It would
  turn the silent `null` into a logged warning.
- **overlaps** `DATA-FORMAT-CONSTANTS-AND-TOOLING`. Its generic serde path (CBOR, YAML) is what gives
  a lossless alternative; it was updated 2026-10-13 with this case rather than filing a duplicate
  (overlap test T4).
- Precedent: `liquers-records/src/formats/ndjson.rs` `scalar_to_json` refuses non-finite values.
  It is a table writer whose columns are typed, so an error there loses no data that a lossy `null`
  would have kept. It stays as it is, and the guide records the difference.

Overlap triage: `grep` of `specs/index.csv` for `json`, `float`, `nan`, `serializ`, `cbor`,
`core/value`, and of every design folder for `try_into_json_value` / `from_f64`. No open item
overlaps strongly.

### Open Questions

Readiness is assessed when Phases 1-4 exist. Automatic fixing is expected to be `not-eligible`: the
fix spans three crates, adds public helpers to `liquers-core`, changes a serialized format, and is
`P0`.

1. **Q1, decided 2026-10-13 by the maintainer:** the policy above. Nested untagged floats are
   written as `null`.
2. **Q2, decided 2026-10-13 by the maintainer:** `try_into_json_value` refuses non-finite floats at
   every depth, rather than mapping nested ones to `null` the way the byte path does. The reasoning
   is in "Why the in-memory conversion still errors" above.
3. **Q3, proposed resolution:** leave `CommandParameterValue::from_float` in
   `liquers-core/src/command_metadata.rs` as it is. It is private and nothing calls it. Its inputs
   are compile-time literals, and the macro already guards them with `unwrap_or`.
4. **Q4, implementation detail, decided in Phase 2:** how `SimpleValue`'s `F64` field gets its
   encoding. A field-level `serialize_with` / `deserialize_with` is the simplest option, and nothing
   else uses the derived serde form (there is no YAML writer, and the YAML reader does not use
   it). The top-level scalar arm of `as_bytes("json")` and the `"F64"` read arm call the same core
   helpers. Helper names are also decided there.

## Phase 2: Architecture

## Phase 3: Examples and Tests

## Phase 4: Implementation Plan
