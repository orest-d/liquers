---
id: VALUE-SERIALIZATION-WRITER
kind: design
title: A writer-based serialization method beside as_bytes
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, lib/value]
issues: [VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER]
created: 2026-10-19
---
# A writer-based serialization method beside as_bytes

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Give every value a synchronous, push-based way to write its encoding into a `std::io::Write`, so
a caller that already has a sink (a file, a socket, a pipe, an encoder) does not have to hold the
whole encoding in a `Vec<u8>` first. `as_bytes` stays, and its bytes do not change.

### Problem Example

```rust
// liquers-lib, feature polars: a 2 GB DataFrame, to be written as CSV to a file
let value = Value::from_polars_dataframe(df);     // ExtValue::PolarsDataFrame
let bytes = value.as_bytes("csv")?;               // the only way in: ~2 GB more resident
std::fs::write("out.csv", &bytes)?;
```

Today: `ExtValue::as_bytes` (`liquers-lib/src/value/mod.rs`) calls
`serialize_dataframe_to_writer(value, format, &mut bytes)` (`liquers-lib/src/polars/serde.rs`), a
writer-based encoder, with a `Vec<u8>` as the writer, because `DefaultValueSerializer`
(`liquers-core/src/value.rs`) has `as_bytes` and `deserialize_from_bytes` only. Peak memory is the
frame plus its full CSV.
Should be: `value.write_to("csv", &mut file)?` streams the CSV into the file in polars' batches,
writing the same bytes `as_bytes` would.

### Scope and Acceptance Criteria

- **AC-1** A DataFrame is written incrementally
  WHEN a polars DataFrame of 100 000 rows is written as `csv` through `write_to`
  THEN the writer receives the encoding in more than one `write` call, none of them the whole
  encoding, and the concatenation equals `as_bytes("csv")`
- **AC-2** Same bytes as `as_bytes`
  WHEN any core `Value` variant is written through `write_to` in every format its `as_bytes` accepts
  THEN the written bytes equal `as_bytes` for that format
- **AC-3** Every existing type has a writer
  WHEN a type that does not override `write_to` (an image, a record view, a test extension) is
  written through it
  THEN it writes exactly `as_bytes`, with no code change in that type
- **AC-4** Errors are typed
  WHEN the format is unsupported, or the writer fails with an I/O error
  THEN `write_to` returns a `SerializationError` (the same message as `as_bytes` for the first, the
  I/O message for the second)
- **AC-5** A state writes like `State::as_bytes`
  WHEN `State::write_to` is called on a state carrying a value error, and on a ready state
  THEN it returns that error without writing, and writes the effective data format's encoding
- **AC-6** Combined values forward
  WHEN a liquers-lib `Value` holding a DataFrame is written through `write_to`
  THEN the extension's incremental override is used (AC-1 holds through `CombinedValue`)

Non-goals: async or pull-based serialization, a store write side (`openbin`), HTTP streaming and
lazy values — all `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE`; a reader-based
deserialization (pairs with `CORE-STORE-OPENBIN-MISSING`); writer overrides for record views and
images (`write_table` and the image encoder return or need an in-memory buffer at HEAD); changing
any in-tree caller, since every in-tree consumer ends in a `&[u8]` store write or a `Vec` HTTP body.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to `DefaultValueSerializer` and
  a `pub` method to `State` (rule 4), across `liquers-core` and `liquers-lib` (rule 6).
- **Leading issue:** **Open design - standalone or folded:** the issue's own updates say to fold it
  into the wider `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` redesign, which has no design.
- **Explanation:** additive and compatible with the redesign; every named symbol was re-read at
  HEAD. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - standalone or folded:** land this now, or only inside the async redesign?
     Recommended: now. For an in-memory value the native encoders (polars `CsvWriter`,
     `ParquetWriter`, `serde_json::to_writer`) are push-based, so this method is the primitive the
     redesign's default stream needs for them anyway; the redesign's "writer built on the stream"
     rule concerns lazy values, which cannot implement a sync method. It has no in-tree consumer
     until that redesign or an embedder uses it.
  2. **Proposed resolution - writer type:** `&mut dyn std::io::Write`, not a generic `W: Write`.
     Keeps the method non-generic, so `CombinedValue` and future `ForeignValue` forwarding stay
     trivial; the dynamic-dispatch cost is per write call, not per byte. Available on wasm32.
  3. **Proposed resolution - which types override now:** core `Value` (`json`) and
     `ExtValue::PolarsDataFrame`; everything else takes the default. A records writer form needs
     `liquers-records` work and belongs with the redesign.

### Design Dependencies

- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue, `L`, no design) — **overlaps**,
  strong (T2 shared contract, T4 its scope covers this one's): kept as a separate design by
  direction of the bulk run; the redesign should list this design as `requires` or absorb it. A
  merge is the maintainer's call (open question 1).
- `record-streams` — origin (complete, frozen, E4): found this while designing; scoped it out.
- `serialization-lossy-write-warnings` — **overlaps**, weak: warnings are a separate method, so
  `write_to` returns `()`; no T1-T4 holds. No order.
- `CORE-STORE-OPENBIN-MISSING` (issue) — **overlaps**, weak: the read side; not required here.

### Consolidated Findings

- The method is additive with a default, so no implementor (`SimpleValue`, `liquers-py`,
  `liquers-web`'s extensions) changes; only overrides gain anything.
- Byte identity with `as_bytes` is the contract (AC-2): the stored version hash
  (`Version::from_content`) is computed over `as_bytes`, and a second encoder that differed would
  split one value into two versions.
- `serialize_dataframe_to_writer` clones the `DataFrame` (cheap: columns are reference-counted) and
  is already generic over `Write`; `&mut dyn Write` satisfies it.
- No in-tree caller changes; the benefit is realized by embedders now and by the redesign later.

## Phase 2: Architecture

### Solution

```rust
// liquers-core/src/value.rs, DefaultValueSerializer — default method
/// Writes the encoding `as_bytes(data_format)` would return, incrementally where the type can.
/// On error the writer may hold a prefix of the encoding.
fn write_to(&self, data_format: &str, writer: &mut dyn std::io::Write) -> Result<(), Error> {
    let bytes = self.as_bytes(data_format)?;
    writer
        .write_all(&bytes)
        .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
}

// liquers-core/src/state.rs, impl State<V>
pub fn write_to(&self, writer: &mut dyn std::io::Write) -> Result<(), Error>;   // value_error check,
                                                                                 // effective_data_format()
```

- **Core `Value`** overrides `write_to`: `"json"` writes with `serde_json::to_writer` (the
  top-level `F64` through `json_f64::to_json`, as `as_bytes` does); every other format falls back
  to the default body, through a public helper `write_as_bytes`, since an override cannot call the
  trait default it replaces.
- **`ExtValue`** (`liquers-lib/src/value/mod.rs`) overrides: `PolarsDataFrame` (feature `polars`)
  calls `serialize_dataframe_to_writer(value, format, writer)`; every other variant uses the
  shared default helper. The `match` is explicit, no `_ =>` (repository rule).
- **`CombinedValue<B, E>`** (`liquers-lib/src/value/extended.rs`) forwards to the half it holds.
- `ExtValue::as_bytes` for `PolarsDataFrame` is unchanged, so the two paths share one encoder.

Rejected: generic `W: Write` (Q2); `as_bytes` implemented on top of `write_to` (no benefit, and the
redesign may invert it again); an async writer (`AsyncWrite`) now — that is the redesign's
contract (pull vs push, wasm `BoxStream` alias) and would pre-empt it; overriding `RecordView`
(needs a `write_table_to` in `liquers-records`).

Known-issue preflight: `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (overlap, not a
blocker); `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT` (no new `unwrap`). Nothing open blocks.

### Changes

- `liquers-core/src/value.rs`: `DefaultValueSerializer::write_to` (default), a `pub fn
  write_as_bytes<V: DefaultValueSerializer>(value: &V, data_format: &str, writer: &mut dyn Write)
  -> Result<(), Error>` helper that the default body also calls, override in
  `impl DefaultValueSerializer for Value`.
- `liquers-core/src/state.rs`: `State::write_to`.
- `liquers-lib/src/value/mod.rs`: `impl DefaultValueSerializer for ExtValue` — `write_to`.
- `liquers-lib/src/value/extended.rs`: `CombinedValue` forwarding.
- No command, registry, store, HTTP or binding change. Sync; no async boundary touched.
- Documents: `specs/reference/VALUE_TYPE_SYSTEM.md` (the serializer's methods; History row,
  `reviewed:`), `specs/guides/SERIALIZATION_GUIDE.md` (a short "writing to a sink" section;
  History row, `reviewed:`), `specs/guides/TYPE_SYSTEM_GUIDE.md` (step 3: `write_to` is optional).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| Override bytes drift from `as_bytes` | same encoder function in both; AC-2 table test, AC-1 equality | high |
| `polars` off: `ExtValue` arm missing | `#[cfg(feature = "polars")]` arm mirrors `as_bytes`; `scripts/check-build-matrix.sh` | high |
| A partial write is mistaken for a complete one | documented: on `Err` the writer holds a prefix; callers own atomicity | medium |
| The redesign wants a different shape | additive default method; removable or re-based without touching callers (none in-tree) | medium |

Recovery: one commit per step; nothing in-tree calls the method, so any step reverts alone.

## Phase 3: Examples and Tests

### Examples

```rust
let mut file = std::fs::File::create("out.csv")?;
value.write_to("csv", &mut file)?;                    // DataFrame: polars batches, no full Vec
state.write_to(&mut std::io::stderr().lock())?;       // effective data format of the state
```

### Tests

Unit, `liquers-core/src/value.rs` `mod tests`:
- `value_write_to_matches_as_bytes` — every variant × every accepted format (the existing
  round-trip table): AC-2
- `value_write_to_unsupported_format_is_serialization_error` — `md` for an `I32`: AC-4
- `value_write_to_reports_io_error` — a writer whose `write` returns `Err`: AC-4
- `default_write_to_writes_as_bytes` — a `Value` format without an override (`txt`): AC-3

Unit, `liquers-core/src/state.rs` `mod tests`:
- `state_write_to_refuses_value_error_and_uses_effective_format`: AC-5

`liquers-lib/src/value/extended.rs` `mod tests`:
- `combined_value_write_to_uses_default_for_test_extension` — `ScalarExtension`: AC-3

New `liquers-lib/tests/value_write_to.rs`, `#![cfg(feature = "polars")]`, with a recording writer
(counts calls, keeps the largest single write):
- `dataframe_write_to_is_incremental_and_identical` — 100 000-row frame, `csv` and `parquet`: AC-1
- `combined_value_forwards_dataframe_write_to` — through `liquers_lib::value::Value`: AC-6
- `image_and_record_view_write_to_equal_as_bytes` — `#[cfg(feature = "image-support")]` /
  `#[cfg(feature = "records")]` per test: AC-3

Run: `cargo test -p liquers-core --lib value:: state::`,
`cargo test -p liquers-lib --test value_write_to`, `cargo test -p liquers-lib --lib value::extended`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/value.rs` — `write_to` default, `write_as_bytes`, `Value` override; the four unit tests — `cargo test -p liquers-core --lib value::`. Rollback: revert the commit.
- [ ] 2. `liquers-core/src/state.rs` — `State::write_to`; its test — `cargo test -p liquers-core --lib state::`.
- [ ] 3. `liquers-lib/src/value/extended.rs` — `CombinedValue::write_to` forwards; `combined_value_write_to_uses_default_for_test_extension` — `cargo test -p liquers-lib --lib value::extended`.
- [ ] 4. `liquers-lib/src/value/mod.rs` `ExtValue` — `write_to` with the `PolarsDataFrame` arm; `liquers-lib/tests/value_write_to.rs` — `cargo test -p liquers-lib --test value_write_to`, then `cargo test -p liquers-lib --no-default-features --lib --tests`.
- [ ] 5. Docs: `VALUE_TYPE_SYSTEM.md`, `SERIALIZATION_GUIDE.md`, `TYPE_SYSTEM_GUIDE.md` (History rows, `reviewed:`); close `VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` with a resolution note; add a line to `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` naming `write_to` as the in-memory push primitive — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (a `cfg(feature = "polars")` arm is added; wasm rows);
`python3 scripts/docs_index.py --check`; diff review for edits outside the four source files, the
new test file and the named documents.
