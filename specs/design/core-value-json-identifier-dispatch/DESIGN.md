---
id: CORE-VALUE-JSON-IDENTIFIER-DISPATCH
kind: design
title: Core Value reads JSON by its type identifier for every core variant
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [core/value]
issues: [CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS]
created: 2026-10-20
---
# Core Value reads JSON by its type identifier for every core variant

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

A core `Value` written as `json` must read back as the variant it was written from, because the
type identifier stored beside the bytes says which variant that is. Today only `F64` and `Bytes`
use it; every other identifier is ignored, and an untagged parse picks the first variant whose
shape fits.

### Problem Example

At HEAD (`liquers-core/src/value.rs`, `impl DefaultValueSerializer for Value`, `deserialize_from_bytes`,
`"json"` arm, fallback `_ => serde_json::from_slice(b)` into the `#[serde(untagged)]` enum):

```rust
let mut record = MetadataRecord::new();
record.title = "notes".into();
let v = Value::Metadata(record);
let bytes = v.as_bytes("json")?;                                   // the declared default format
let back = Value::deserialize_from_bytes(&bytes, "Metadata", "json")?;
// today:     Value::Object({"title": Text("notes"), "log": Array([]), …})
// should be: Value::Metadata(record), identifier "Metadata"

Value::deserialize_from_bytes(b"5", "I64", "json")?;   // today I32(5); should be I64(5)
```

The variant order of `enum Value` decides: `Object` precedes `Metadata`, `Recipe`,
`CommandMetadata` and `Query` (all serialize as maps); `Array` precedes `AssetInfo` and `Key` (a
`Key` is a newtype over `Vec<ResourceName>`, so it serializes as an array, not a map as the issue
says); `I32` precedes `I64`. All of these types declare `json` in `Value::type_descriptions()`, and
for `Metadata`, `AssetInfo`, `Recipe` and `CommandMetadata` it is the default format. Persisting
any of them as a keyed asset and reloading it through the fast track returns a different type.
Reproduction confirmed by reading the arm and the enum at HEAD (last change to the file:
`8fde04a`); no test covers these identifiers (`vts7.2`,
`scalar_identifiers_round_trip_through_the_serializer`, uses `I64(-9_000_000_000)`, which does
not fit an `i32`).

### Scope and Acceptance Criteria

- **AC-1** Structured variants round-trip
  WHEN a non-empty `Metadata`, `AssetInfo`, `Recipe`, `CommandMetadata`, `Query` or `Key` value is
  written with `as_bytes("json")` and read back with its own identifier
  THEN the value read back equals the original and reports the same identifier
- **AC-2** Integers keep their width
  WHEN `Value::I64(5)` and `Value::I32(5)` are written as `json` and read back with their identifiers
  THEN they read back as `I64(5)` and `I32(5)`
- **AC-3** Untyped JSON keeps shape inference
  WHEN JSON is read with an empty identifier, or with an identifier core `Value` does not own
  THEN it is read by shape, exactly as at HEAD
- **AC-4** A mismatched payload is refused
  WHEN bytes whose JSON shape does not fit the declared core identifier are read (`[1,2]` as
  `Metadata`, `"x"` as `I64`)
  THEN `deserialize_from_bytes` returns a `SerializationError` naming the identifier, instead of a
  value of another variant
- **AC-5** Every declared core JSON pair round-trips
  WHEN each core identifier whose `TypeInfo` declares `json` is written and read as `json`
  THEN the identifier read back equals the identifier written

Non-goals: nested values (an `I64` inside an `Array` still reads as `I32`; inside an untagged
container JSON carries no type, as `SERIALIZATION_GUIDE.md` documents); `liquers-lib`'s
`SimpleValue`, which already dispatches its structured identifiers and has the mirror defect for
`I32` (filed as `SIMPLE-VALUE-I32-JSON-READS-BACK-AS-I64`); any change to what `as_bytes` writes.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — bug fix in one function of `liquers-core/src/value.rs` that
  restores the documented "the identifier is the dispatch key" behaviour
  (`VALUE_TYPE_SYSTEM.md` §Two axes); no `pub` item, format or registry change.
- **Leading issue:** None.
- **Explanation:** the fix follows the pattern `SimpleValue::deserialize_from_bytes` already uses
  for the same identifiers; every payload type derives `Deserialize` and `PartialEq`.
- **Open questions:**
  1. **Implementation detail - strict for shape-typed identifiers too:** `None`, `Bool`, `Text`,
     `Array` and `Object` are also read as their own payload type, so `5` read as `Text` is
     refused rather than returned as `I32`. The identifier is documented as the dispatch key, and
     the text arm already refuses an identifier it cannot read; a mismatch is corrupted data.

### Design Dependencies

- `core-value-enum-oversized` — **required-by**: its AC-3 test
  (`large_variants_serialize_as_their_payload`) asserts that core `Metadata`, `Recipe`,
  `CommandMetadata` and `Query` read back equal through `deserialize_from_bytes` with their
  identifier, which fails at HEAD and passes only after this design. Once both land, the arms
  added here wrap their payload in `Arc` (whichever lands second adapts the four arms). Kept
  separate under overlap exclusion E1: this design is eligible for automatic fixing, that one is not.
- `serialization-lossy-write-warnings`, `value-serialization-writer` — **overlaps**, weak: both add
  methods to the same `impl`, not to this arm. No order.
- `data-format-vocabulary` — **overlaps**, weak: would later replace the `"json"` literal with a
  constant. No order.

## Phase 2: Architecture

### Solution

In `liquers-core/src/value.rs`, `impl DefaultValueSerializer for Value`, the `"json"` arm of
`deserialize_from_bytes` matches every core identifier explicitly:

| Identifier | Read as |
|---|---|
| `"F64"`, `"Bytes"` | unchanged (`json_f64::from_json`; `Vec<u8>`) |
| `"None"` | `()` (accepts `null`) → `Value::None` |
| `"Bool"`, `"I32"`, `"I64"`, `"Text"` | `bool`, `i32`, `i64`, `String` |
| `"Array"`, `"Object"` | `Vec<Value>`, `BTreeMap<String, Value>` (elements still by shape) |
| `"Metadata"`, `"AssetInfo"`, `"Recipe"`, `"CommandMetadata"`, `"Query"`, `"Key"` | `MetadataRecord`, `Vec<AssetInfo>`, `Recipe`, `CommandMetadata`, `Query`, `Key` |
| `""` and any other identifier | `serde_json::from_slice::<Value>` (untagged, as at HEAD) |

A private generic helper keeps each arm one line and the error uniform:

```rust
fn json_payload<T: serde::de::DeserializeOwned>(b: &[u8], type_identifier: &str) -> Result<T, Error>
// Err: Error::from_error(ErrorType::SerializationError,
//      format!("JSON error reading {type_identifier} in from_bytes: {e}"))
```

The `F64` and `Bytes` arms keep their existing messages. The fallback arm keeps its message. No
`Error::new` is introduced. Sync, no allocation beyond the payload.

Rejected: a tagged JSON form for core `Value` (changes stored bytes and the HTTP JSON every
client reads; `liquers-lib` already has the tagged form); keeping untagged inference for the
shape-typed identifiers (returns a value of another type for a mismatched payload, which the
identifier contract forbids).

Known-issue preflight: `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` (P3, the asset layer picks
`bin` when no format is declared) is independent: it decides which arm runs, not what this arm
does. Nothing open blocks.

### Changes

- `liquers-core/src/value.rs`: the `"json"` arm of `Value::deserialize_from_bytes`; private
  `json_payload`; the comment above the arm (it already explains the identifier as the
  discriminator, and now names every variant).
- Tests in the same file's `mod tests` (Phase 3).
- `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading: one sentence — core `Value` reads every core
  identifier in `json` as that variant's payload, and only an empty or foreign identifier by shape;
  History row, `reviewed:`.
- No command, registry, store, binding or `liquers-lib` change.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/value.rs`; the reference document |
| Existing tests | `vts7.2`, `json_f64_*`, `markdown_is_text_only` keep passing (no arm they use changes) |
| Compatibility / data | Stored core-`Value` JSON with a matching identifier now reads as its own type (the intended fix). A stored file whose identifier disagrees with its JSON shape now fails to deserialize instead of yielding another type; the asset layer treats that as corrupted and recomputes |
| Consumers | Only environments whose value type is core `Value` (core and `liquers-axum` tests, generic code); `liquers-lib`, `liquers-py`, `liquers-web` use `SimpleValue`-based values |
| Recovery | Revert the one commit |
| Certainty | High |

## Phase 3: Examples and Tests

### Examples

```rust
let q = parse_query("-R/data/x.csv/-/ns-pl/head-5")?;
let back = Value::deserialize_from_bytes(&Value::Query(q.clone()).as_bytes("json")?, "Query", "json")?;
assert_eq!(back, Value::Query(q));
assert!(Value::deserialize_from_bytes(b"[1,2]", "Metadata", "json").is_err());
assert_eq!(Value::deserialize_from_bytes(b"{\"a\":1}", "", "json")?.identifier(), "Object");
```

The query in the example validates (`liquers-validate`: resource `data/x.csv`, then `ns-pl/head-5`; a parse-level example, so no command needs registering).

### Tests

Unit, `liquers-core/src/value.rs` `mod tests`:

- `structured_identifiers_round_trip_as_json` — non-empty samples: a `MetadataRecord` with a
  title and a log entry, `vec![AssetInfo::new()]` with a filename, a `Recipe::new` of the
  example query, `CommandMetadata::new("test")`, the example query, `parse_key("data/x.csv")`;
  each read back equal with the same identifier: AC-1
- `scalar_identifiers_round_trip_through_the_serializer` (`vts7.2`) — add `(Value::I64(5),
  &["json", "txt"])` beside the existing large `I64`: AC-2
- `untyped_json_reads_by_shape` — `""` and `"test.Foreign"` with `{"a":1}`, `[1]`, `5`: `Object`,
  `Array`, `I32`, as at HEAD: AC-3
- `mismatched_json_payload_is_refused` — `[1,2]` as `Metadata`, `"x"` as `I64`, `5` as `Text`:
  `ErrorType::SerializationError`, message names the identifier: AC-4
- `every_declared_json_identifier_round_trips` — for each `TypeInfo` in `Value::type_descriptions()`
  declaring `json`, the matching `sample_values()` entry round-trips with its identifier: AC-5

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib value::tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/value.rs` `mod tests` — add the five tests (four fail at HEAD; `untyped_json_reads_by_shape` passes and guards AC-3) — `cargo test -p liquers-core --lib value::tests`. Rollback: remove the tests.
- [ ] 2. `liquers-core/src/value.rs` `Value::deserialize_from_bytes` — explicit identifier arms and `json_payload` — `cargo test -p liquers-core --lib value::tests` all pass. Rollback: revert the commit.
- [ ] 3. `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading sentence, History row, `reviewed:`; close `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS` (`status: closed`, resolution naming the tests) — `python3 scripts/docs_index.py --check`.

### Validation

`CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-axum
--lib` (its tests use core `Value`); `cargo fmt --check` on the changed file; diff review: only
`value.rs`, the reference document, the issue and the index.
