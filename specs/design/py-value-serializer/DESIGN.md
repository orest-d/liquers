---
id: PY-VALUE-SERIALIZER
kind: design
title: liquers-py's value serializer delegates to the core codec
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [py, core/value]
issues: [PY-VALUE-SERIALIZER-IS-A-STUB]
created: 2026-10-19
---
# liquers-py's value serializer delegates to the core codec

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-19. Phases 1-4 are written but not
approved, and nothing is implemented.

## Phase 1: High-Level Design

### Purpose

`impl DefaultValueSerializer for Value` in `liquers-py/src/value.rs` writes only `txt`/`html` for
eight scalar variants and reads nothing. Every shared variant already has a working codec in
`liquers-core` (`impl DefaultValueSerializer for liquers_core::value::Value`) under the same type
identifier, and `fvt6.3` (`shared_variants_match_the_core_identifiers`) promises that a store
written from Python is readable from Rust. The Python serializer should be that codec, reached
through a conversion, so the two cannot diverge.

### Problem Example

```rust
// liquers-py/src/value.rs, mod tests (GIL-free)
let v = Value::Object { value: BTreeMap::from([("a".into(), Value::I32 { value: 1 })]) };
v.default_extension();                       // "json"
v.as_bytes("json");                          // today: Err("Unsupported format json")
Value::deserialize_from_bytes(b"{\"a\":1}", "Object", "json"); // today: Err, for every format
```

Should be: `as_bytes("json")` returns `{"a":1}` — the same bytes `liquers_core::value::Value`
writes for the same object — and reading them back with identifier `Object` yields an `Object`
that re-encodes to the same bytes. `Value::Text` written as `txt` must keep producing exactly the
bytes it produces today.

### Scope and Acceptance Criteria

- **AC-1** Every declared format writes
  WHEN `as_bytes(f)` is called on a sample of each non-`Py` variant, for each format `f` its
  `TypeInfo` declares in `Value::type_descriptions()`
  THEN it returns `Ok`
- **AC-2** Round trip where core round-trips
  WHEN bytes written in a declared format are read with `deserialize_from_bytes(bytes, identifier, f)`
  for the cases core's own `vts7.2` round-trips plus `Array`/`Object` as `json` and `Query`/`Key`
  as `txt`
  THEN the result has the same identifier and re-encodes to the same bytes; every other case reads
  back exactly what core reads back (AC-3), and is listed in the test as a known core gap
- **AC-3** Byte-compatible with core
  WHEN a shared variant is written by liquers-py and the equivalent `liquers_core::value::Value`
  is written by core, in the same format
  THEN the bytes are identical, and each side reads the other's bytes
- **AC-4** Declarations match core
  WHEN `Value::type_descriptions()` is compared with `liquers_core::value::Value::type_descriptions()`
  THEN every shared identifier carries the same type name, defaults and data formats, and the only
  extra entry is `py.Object`
- **AC-5** Python objects refuse cleanly
  WHEN `as_bytes` is called on `Value::Py`, or on an `Array`/`Object` containing one, or
  `deserialize_from_bytes` is called with identifier `py.Object`
  THEN it returns a `SerializationError` naming `py.Object`, never a panic; `py.Object` declares
  no data format, so the asset layer stores it as metadata only (unchanged)
- **AC-6** No default arm
  WHEN the serializer's `match` statements are read
  THEN none has a `_ =>` arm over `Value` (CLAUDE.md "Match Statements")

Non-goals: a byte form for `Value::Py` (pickling is a separate, security-relevant decision, filed
as `PY-OBJECT-VALUE-HAS-NO-BYTE-FORM`); a format vocabulary or generic serde path
(`DATA-FORMAT-CONSTANTS-AND-TOOLING`); a Python store or evaluation path (the Python
`Environment` cannot evaluate yet, `PY-ENVIRONMENT-CANNOT-EVALUATE`), so the codec is exercised by
unit tests, not through a store.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — a bug fix restoring each type's declared default format (the
  defaults already say `json`); private helper functions only; no `pub` signature, command,
  dependency or Python-visible API change; the bytes are core's existing formats; one crate
  (`liquers-py`).
- **Leading issue:** None
- **Explanation:** core's codec already defines every shared format and the identifiers already
  agree (`fvt6.3`), so delegation has no contract left to choose; `Py` keeps today's
  metadata-only behaviour and its byte form is split off.
- **Open questions:** None. `Py` is scoped out on the guidance in
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` ("Pickle may be an opt-in codec, never an implicit
  trusted interchange format").

### Design Dependencies

- `py-value-arc-payloads` (in_review) — **overlaps**: edits the `MetadataRecord`/`Recipe`/
  `CommandMetadata`/`Query` wrappers this design converts. No order; whichever lands second
  writes the conversion as `(*inner).clone()` / `Wrapper::from(core)` instead of field access.
- `py-orphan-modules` (in_review) — **overlaps**: same crate, different files; no order.
- `DATA-FORMAT-CONSTANTS-AND-TOOLING` (feature, no design) — **overlaps**: would replace core's
  hand-written arms; delegation means liquers-py inherits that change for free.
- `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS` (issue, filed by this design, no design)
  — **overlaps**: core reads `Metadata`, `AssetInfo`, `Recipe`, `CommandMetadata`, `Query`, `Key`
  and small `I64` written as `json` back as another variant, because its untagged JSON arm
  discriminates only `F64` and `Bytes`. Not required first: delegation inherits the fix, and
  `pyser02` lists these cases as known gaps until it lands.

### Consolidated Findings

- Core's `type_descriptions()` matches liquers-py's identifier, type name and defaults for all 15
  shared variants (both lists read at HEAD), so `type_descriptions()` can be built *from* core's
  list plus the `py.Object` entry; declaration and codec then come from one place (AC-4).
- The conversion is a deep clone per write. Acceptable: `as_bytes` allocates the whole encoding
  anyway, and Python values are not on a hot path.
- The existing test `declared_formats_are_ones_the_serializer_accepts` hard-codes `txt`/`html` and
  is replaced, not loosened.
- Core's JSON read is identifier-blind for the structured variants (filed as
  `CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS`); this design deliberately does not patch
  around it in liquers-py, so the two codecs stay one.
- No store round trip is possible from Python today (`PY-ENVIRONMENT-CANNOT-EVALUATE`), so the
  user-visible effect lands when that does; the tests here stand in for it.

## Phase 2: Architecture

### Solution

Delegate. In `liquers-py/src/value.rs` add two private functions and rewrite the trait impl:

```rust
type CoreValue = liquers_core::value::Value;
fn to_core_value(v: &Value) -> Result<CoreValue, Error>;   // Py (anywhere inside) -> SerializationError
fn from_core_value(v: CoreValue) -> Value;                  // total: every core variant has a py twin
impl DefaultValueSerializer for Value {
    fn as_bytes(&self, f: &str) -> Result<Vec<u8>, Error> { to_core_value(self)?.as_bytes(f) }
    fn deserialize_from_bytes(b: &[u8], id: &str, f: &str) -> Result<Self, Error> {
        if id == PY_OBJECT_TYPE_IDENTIFIER { return Err(/* no byte form */); }
        CoreValue::deserialize_from_bytes(b, id, f).map(from_core_value)
    }
}
```

Both conversions are exhaustive matches (16 py arms, 15 core arms), recursive for `Array`/`Object`.
Wrappers convert through their public fields: `MetadataRecord.inner`, `Recipe.inner`,
`CommandMetadata.0`, `parse::Query.0`, `query::Key.0`, `metadata::AssetInfo.inner`.
`type_descriptions()` becomes `CoreValue::type_descriptions()` plus the existing `py.Object`
`TypeInfo` (unchanged: `pickle` defaults, no data formats). Errors use
`Error::from_error(ErrorType::SerializationError, …)`, never `Error::new` (the two existing
`Error::new` calls go away with the stub).

Rejected: hand-writing a JSON arm per variant (re-creates the drift the issue describes, and must
replicate core's `json_f64` and identifier-discriminated `Bytes`/`F64` reading); `Serialize` on the
py enum (`Py<PyAny>` has no serde form and the `#[pyclass]` enum's shape differs from core's
untagged one, so bytes would not match core).

Known-issue preflight: `PY-VALUE-ENUM-OVERSIZED` (design `py-value-arc-payloads`) — touches the
same wrappers, does not block. `DATA-FORMAT-CONSTANTS-AND-TOOLING` — not required. Nothing open
blocks. No command namespace involved.

### Changes

- `liquers-py/src/value.rs`: `to_core_value`, `from_core_value` (new, private);
  `DefaultValueSerializer for Value` rewritten; `ValueInterface::type_descriptions` built from
  core's list; the comment above it updated (no longer "this crate's serializer is a stub").
- Tests in the same file's `mod tests` (Phase 3). No other file; no command, registry, Python stub
  or document change. Close `PY-VALUE-SERIALIZER-IS-A-STUB`.

### Risks

| Risk | Note | Certainty |
|---|---|---|
| Files | `liquers-py/src/value.rs` only | high |
| A wrapper field is private | all six are `pub` at HEAD (`metadata.rs:242,547`, `recipes.rs:6`, `query.rs:288,495`, `command_metadata.rs`) | high |
| Text bytes change | core's `txt` arm is the same code as the stub's for the eight scalar variants; AC-3 test pins it | high |
| Concurrent `py-value-arc-payloads` | conversions are 6 lines to adjust; compiler-guided | high |
| Build | liquers-py tests need `--no-default-features --features async_store` (links libpython). All tests but `pyser06` are GIL-free; `pyser06` initializes an interpreter itself, which the existing tests never did — if that fails in the test binary, move its two assertions to a pytest file under maturin (`liquers-py/README.md`) and say so in the PR | medium |

Recovery: one commit; revert restores the stub.

## Phase 3: Examples and Tests

### Examples

The Problem Example is test `pyser02`. Python-visible behaviour does not change until a Python
environment can store values.

### Tests

In `liquers-py/src/value.rs` `mod tests`, reusing `samples()` (every variant but `Py`):

- `pyser01_every_declared_format_writes` — AC-1
- `pyser02_round_trip_where_core_round_trips` — identifier and re-encoded bytes equal for the
  AC-2 cases; the core-gap cases (`CORE-VALUE-JSON-READ-IGNORES-STRUCTURED-IDENTIFIERS`) in an
  explicit list, asserted only to read back what core reads back — AC-2
- `pyser03_bytes_match_core_both_ways` — each sample against its `to_core_value` twin, every
  declared format; reading core bytes with liquers-py and vice versa — AC-3
- `pyser04_descriptions_match_core` — replaces `declared_formats_are_ones_the_serializer_accepts`
  — AC-4
- `pyser05_python_object_read_is_refused` — GIL-free: `deserialize_from_bytes(_, "py.Object",
  "pickle")` is a `SerializationError` naming `py.Object`, and its `TypeInfo` declares no data
  format — AC-5
- `pyser06_python_object_write_is_refused` — needs an interpreter: `pyo3::prepare_freethreaded_python()`,
  then inside `Python::with_gil` build `Value::Py` and an `Array` containing it; both `as_bytes("json")`
  are a `SerializationError` naming `py.Object` — AC-5
- AC-6 is a review item checked in Phase 4 step 2 (`grep -n "_ =>"` over the new functions)
  — AC-6

Existing tests that must stay green: `type_descriptions_match_identifier`,
`shared_variants_match_the_core_identifiers`, `the_registry_describes_every_variant`.

Run: `cargo test -p liquers-py --lib --no-default-features --features async_store pyser`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-py/src/value.rs` `mod tests` — add `pyser01`-`pyser06`; `pyser01`-`pyser03` fail at HEAD — `cargo test -p liquers-py --lib --no-default-features --features async_store pyser`. Rollback: delete the tests.
- [ ] 2. `liquers-py/src/value.rs` — add `to_core_value` / `from_core_value`, rewrite `DefaultValueSerializer for Value`; no `_ =>` arm (AC-6, checked with `grep -n "_ =>" liquers-py/src/value.rs`) — same command.
- [ ] 3. `liquers-py/src/value.rs` `type_descriptions` — build from `CoreValue::type_descriptions()` + `py.Object`; delete `declared_formats_are_ones_the_serializer_accepts` — `cargo test -p liquers-py --lib --no-default-features --features async_store`.
- [ ] 4. Records — close `PY-VALUE-SERIALIZER-IS-A-STUB` with a resolution note — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-py --lib --no-default-features --features async_store`; `cargo check -p
liquers-py --lib` (default features, the wheel configuration; prefix
`PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1` on CPython 3.13+); a diff review confirming only
`liquers-py/src/value.rs` and the issue/index changed and no `println!`.
