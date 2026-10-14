---
title: Choosing a Serialization Format
kind: guide
audience: both
area: [core/value, lib/value]
reviewed: 2026-10-13
---
# Choosing a serialization format

This guide covers what each data format preserves when a value is written to a store and read back,
and which format to pick when the difference matters. Format selection itself is in
[`TYPE_SYSTEM_GUIDE.md`](TYPE_SYSTEM_GUIDE.md) §"Choosing a data format at write time", and the
type model is in [`reference/VALUE_TYPE_SYSTEM.md`](../reference/VALUE_TYPE_SYSTEM.md).

**The short rule: JSON is a convenience format, not an archival one.** It is the default for most
base values because every tool can read it. It cannot represent every float, and when a value has
no type of its own inside JSON, the type is lost. When a stored result must come back exactly, do
not rely on JSON.

## Non-finite floats (NaN, +∞, −∞)

JSON has no NaN and no infinity (RFC 8259 §6). `serde_json` writes `null` for them without an error,
and it cannot read `NaN`, `null` or `"NaN"` into an `f64`. Liquers resolves this in one place,
`liquers_core::value::json_f64`, and applies one policy:

| Where the float is | JSON written | JSON read |
|---|---|---|
| `try_into_json_value` (in memory), at any depth | — a `ConversionError` | — |
| A top-level `F64`, core `Value` or `SimpleValue` | `"NaN"`, `"Infinity"`, `"-Infinity"` | identifier `F64`: a number, one of those strings, or `null` (→ NaN) |
| Inside a `SimpleValue` / `liquers_lib::value::Value` container | the same strings | the same |
| Inside a core `Value::Array` / `Value::Object` | **`null`** | **`Value::None`** |

- **The strings** are the tokens of the protobuf JSON mapping for `float` / `double`, the most widely
  used lossless convention. Other readers still see valid JSON.
- **Reading `null` as NaN** accepts what `serde_json`, JavaScript (`JSON.stringify`) and pandas
  (`to_json`) write. It cannot recover the sign of an infinity.
- **The identifier decides.** `"NaN"` read as `F64` is a float; read as `Text` it stays text. Nothing
  guesses a float from a string.
- **`liquers-lib` values are lossless at any depth**, because their JSON form is tagged
  (`{"F64":{"value":"NaN"}}`). Files written before 2026-10-13 hold `null` there, and they now
  read as NaN. Earlier they read as an unrelated `Object`.
- **`try_into_json_value` always refuses.** A `serde_json::Value` cannot hold the float. A string
  would bind as text and `null` as `None`, for example when a query link is bound as a command
  parameter. A refused conversion loses nothing: the caller can map the float itself.

### Why nested core floats become `null`, and what that costs

A float nested in a core `Value` container has no type in JSON. That form is untagged: `[1.0, null]`
is all there is. The options were to refuse the write or to write `null`. Liquers writes `null`. A
refused write stores nothing, because the asset falls back to metadata-only storage and is
recomputed later, and for the result of an expensive operation that is worse than a few `null`s.

This is a deliberate compromise with real costs:

- **The loss is silent.** A cached value can differ from the freshly computed one (`None` instead of
  NaN), depending on whether it came from the cache. No warning is recorded, because serialization
  has no channel for one. This is tracked as
  [`SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE`](../issues/SERIALIZATION-CANNOT-REPORT-A-LOSSY-WRITE.md),
  whose preferred fix is part of the serialization redesign
  ([`VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`](../issues/VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE.md)).
- **It makes JSON look safer than it is.** Because the write succeeds, nothing tells a user that
  their data changed.

The table writer in `liquers-records` (`scalar_to_json`) refuses non-finite values instead. Its
columns are typed, so there is no "type-less" position to fill with `null`, and refusing loses no
information a `null` would have kept.

## Float fidelity by format

| Format | A scalar `F64` NaN/±∞ | Inside a core container | Inside a `liquers-lib` container |
|---|---|---|---|
| `json` | lossless (token) | **lost** (`null` → `None`) | lossless (token) |
| `txt` | lossless (`NaN`, `inf`, `-inf`) | — not writable | — not writable |
| `yaml` | — | — not supported | reading turns `.nan` into `None`; no writer |
| CBOR, MessagePack, pickle | — media types only, not implemented for base values | | |

**When fidelity matters:**

- Store a single float as `txt`, or use a `liquers-lib` value as `json`.
- For a core `Value` container there is no lossless format yet. The generic serde path planned in
  [`DATA-FORMAT-CONSTANTS-AND-TOOLING`](../issues/DATA-FORMAT-CONSTANTS-AND-TOOLING.md) is what
  brings CBOR (native NaN/∞) and a proper YAML path.
- For tabular data, use a record view in a binary table format (`ipc`, `parquet`). These keep
  floats exactly.

## For implementers

- A value type that writes floats to JSON uses `json_f64`. Use `to_number` for a strict in-memory
  conversion, and `to_json` / `from_json` for the byte path. On a field that is always a float, use
  `#[serde(with = "liquers_core::value::json_f64")]`.
- **Never put that attribute on a variant of an untagged enum.** The token would read back as text.
  Handle the top level explicitly instead, as core `Value` does in `as_bytes` and
  `deserialize_from_bytes`.
- Never `unwrap` `serde_json::Number::from_f64`. It returns `None` exactly for the values this
  guide is about.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-13 | Created: the non-finite float policy, its compromise, and float fidelity by format. | phase-5, `design/value-to-json-non-finite-float/` |
