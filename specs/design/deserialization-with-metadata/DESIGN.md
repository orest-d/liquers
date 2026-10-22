---
id: DESERIALIZATION-WITH-METADATA
kind: design
title: Deserialization of a stored value can consult the metadata stored with it
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/assets, lib/value]
issues: [DESERIALIZATION-RECEIVES-NO-METADATA]
created: 2026-10-20
---
# Deserialization of a stored value can consult the metadata stored with it

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

A stored value is read back from three facts: bytes, a type identifier and a data format. The
metadata record stored beside the bytes can hold more (with `metadata-application-attributes`, an
application's own facts), but the reader never sees it. Pass it through, defaulted so that no
existing value type changes, and use it for the first consumer: a record view stored in a
schema-less table format reads back with the schema recorded in its metadata.

### Problem Example

A record view with schema `id: Int (Id role)`, `code: Text`, `amount: Float` and rows
`(1, "123", 2.0)`, `(2, "456", 3.0)` is stored at `data/t.csv` with identifier `RecordView` and
format `csv`. With `metadata-application-attributes` in place, its record also carries
`attributes["records.schema"]` = that schema. Reloading (fast track, or `get_any_status`):

- **Today:** `deserialize_stored_value` (`liquers-core/src/assets.rs`) calls
  `E::Value::deserialize_from_bytes(binary, "RecordView", "csv")`; the metadata it holds is not
  passed. `ExtValue::deserialize_from_bytes` (`liquers-lib/src/value/mod.rs`, arm `"RecordView"`)
  reads with `ReadSchema::Infer`: `code` becomes `Int`, `amount` becomes `Int`, `id` loses its Id role.
- **Should be:** the reader receives the stored metadata, finds `records.schema`, and reads with
  `ReadSchema::Declared`: the batch's schema equals the one written, roles included.

Confirmed at HEAD: the only production caller of `deserialize_from_bytes` is
`deserialize_stored_value`, called from `AssetData::try_fast_track` and
`DefaultAssetManager::get_any_status`, both of which hold the stored `Metadata`; the trait
(`DefaultValueSerializer`, `liquers-core/src/value.rs`) has no parameter for it.

### Scope and Acceptance Criteria

- **AC-1** The schema in metadata is honoured
  WHEN a `RecordView` stored as `csv` (also `tsv`, `ndjson`, `json`, `md`) with a `records.schema`
  attribute is loaded through the asset manager
  THEN the view's schema equals the attribute's schema, field types and roles included
- **AC-2** No attribute, no change
  WHEN the same view is stored without the attribute, or with legacy metadata
  THEN it is read with the inferred schema, exactly as at HEAD
- **AC-3** Self-describing formats keep their own schema
  WHEN a `RecordView` stored as `ipc` or `parquet` carries a `records.schema` attribute
  THEN the file's embedded schema is used and the attribute is ignored
- **AC-4** A wrong schema is a failed read
  WHEN the attribute is not a valid schema, or the bytes do not parse under it
  THEN the deserialization returns an error (the fast track then recomputes, as for any corrupted entry)
- **AC-5** Existing value types are untouched
  WHEN a value type that does not override the new method (core `Value`, `SimpleValue`, a test
  extension) is loaded through the asset manager
  THEN it is read exactly as `deserialize_from_bytes` reads it
- **AC-6** Combined values forward
  WHEN `liquers_lib::value::Value` reads with metadata
  THEN the identifier dispatch is unchanged, and the half that reads receives the metadata

Non-goals: **writing** the attribute when a view is serialized (filed as
`RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE`); a metadata-aware read outside the asset layer
(`ns-pl` input conversion, `to_record`); any change to which identifier or format is chosen.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to `DefaultValueSerializer`
  (rule 4) and spans `liquers-core` and `liquers-lib` (rule 6).
- **Leading issue:** **Open design - write side:** without a producer the read hook only serves
  values whose record an application or a command annotated (via `set_attribute`). Phases 3-4
  assume the recommended answer.
- **Explanation:** one defaulted static method, one private call site with the metadata already in
  hand, and one override. Requires `metadata-application-attributes` for the attribute accessors.
- **Open questions:**
  1. **Open design - write side in a separate design:** recommended yes. Recording the schema on
     write means a value contributing metadata at serialization, at the same five asset-layer
     sites `serialization-lossy-write-warnings` edits; it belongs with that hook (filed as
     `RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE`), not here.
  2. **Proposed resolution - attribute name:** `records.schema`, holding `RecordSchema`'s serde form
     (the `<application>.<name>` convention of `metadata-application-attributes`, whose own example
     uses this name), as a `pub const RECORD_SCHEMA_ATTRIBUTE` in `liquers-lib/src/records/`.
  3. **Proposed resolution - a bad attribute fails the read:** a declared fact that does not parse,
     or does not fit the bytes, is treated as corruption (recompute), consistent with
     `ReadSchema::Declared` being strict. Alternative: fall back to inference with a warning,
     which hides a wrong schema.
  4. **Implementation detail - parameter type:** `&Metadata`, the type both call sites hold; a
     legacy record carries no attributes, so it reads as AC-2.

### Design Dependencies

- `metadata-application-attributes` — **requires**: provides `attributes` and `attribute_as`; the
  core hook (steps 1-2) can land first, the `RecordView` override (step 3) cannot.
- `serialization-lossy-write-warnings` — **overlaps**, weak: edits `serialize_*` sites, this edits
  `deserialize_stored_value`; the write-side follow-up would share its sites.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue, other cluster) — **overlaps**, weak:
  a redesigned reader should keep a metadata argument; no order.
- `data-format-vocabulary` — **overlaps**, weak: it adds a write-only branch to the same
  `deserialize_stored_value` and a parameter to `degraded_reason`; different statements (T3 judged
  not to hold), the second to land rebases.
- `parquet-read-declared-schema` — **overlaps**, weak: Parquet reads keep `Infer` here (AC-3), so
  its declared-schema path is not exercised.

## Phase 2: Architecture

### Solution

`liquers-core/src/value.rs`, `DefaultValueSerializer` — a defaulted static method:

```rust
/// As [`Self::deserialize_from_bytes`], with the metadata stored beside the bytes. The asset
/// layer calls this one. The default ignores `metadata`.
fn deserialize_from_bytes_with_metadata(
    b: &[u8],
    type_identifier: &str,
    data_format: &str,
    metadata: &crate::metadata::Metadata,
) -> Result<Self, Error> {
    let _ = metadata;
    Self::deserialize_from_bytes(b, type_identifier, data_format)
}
```

`liquers-core/src/assets.rs`: private `deserialize_stored_value` gains `metadata: &Metadata` and
calls the new method; `try_fast_track` and `get_any_status` pass the record they already read
(before `verify_store_read`, which only needs it afterwards). The `bytes`/degraded checks before the
call are unchanged.

`liquers-lib/src/value/extended.rs`, `CombinedValue<B, E>`: the current body moves into
`deserialize_from_bytes_with_metadata`, calling `E::…_with_metadata` / `B::…_with_metadata`;
`deserialize_from_bytes` becomes a call to it with `&Metadata::new()`. Dispatch order unchanged.

`liquers-lib/src/value/mod.rs`, `ExtValue`: override `deserialize_from_bytes_with_metadata`. For
`"RecordView"` (under `#[cfg(feature = "records")]`) it reads the schema; every other identifier
calls `Self::deserialize_from_bytes`. The current `"RecordView"` arm body moves to a private
`fn read_record_view(b: &[u8], fmt: &str, schema: ReadSchema) -> Result<ExtValue, Error>`, used by
both methods; `TableFormat` is matched explicitly as today:

| `TableFormat` | Schema used |
|---|---|
| `Csv`, `NdJson`, `Json`, `Markdown` | `Declared(&s)` when the attribute is present, else `Infer` |
| `Ipc`, `Parquet` | `Infer` (the embedded `liquers.schema` wins, AC-3) |
| `Html` | refused by `read_table`, as today |

Reading the attribute: `Metadata::MetadataRecord(r) => r.attribute_as::<RecordSchema>(RECORD_SCHEMA_ATTRIBUTE)?`,
`Metadata::LegacyMetadata(_) => None` (explicit match, no `_ =>`). A conversion failure propagates
(AC-4).

Rejected: a metadata-aware hook only in `liquers-lib` (the asset layer is the only place that has
both the bytes and the record, and it is generic over `E::Value`); changing
`deserialize_from_bytes`' signature (every implementor and test in five crates edits for one
consumer); passing only the attribute map (ties core to attributes and loses `title`, `media_type`).

Known-issue preflight: `STORED-VALUE-WITHOUT-DATA-FORMAT-READ-AS-BIN` (which format reaches the
reader) is independent. `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` unaffected. Nothing blocks.

### Changes

- `liquers-core/src/value.rs`: the trait method. `liquers-core/src/assets.rs`:
  `deserialize_stored_value` and its two callers.
- `liquers-lib/src/value/extended.rs`: `CombinedValue` method moved and forwarded.
- `liquers-lib/src/value/mod.rs`: `ExtValue` override, `read_record_view`.
- `liquers-lib/src/records/mod.rs`: `pub const RECORD_SCHEMA_ATTRIBUTE: &str = "records.schema";`.
- No command, registry, HTTP or binding change; `liquers-py` and `liquers-web` inherit the default.
  Sync throughout. `cfg(feature = "records")` on the `RecordView` branch and the constant.
- Documents: `specs/reference/VALUE_TYPE_SYSTEM.md` §Reading (the asset layer passes the stored
  metadata; the `records.schema` rule); `specs/reference/RECORD_STREAMS.md` (reading a stored view:
  schema from metadata for the text formats); `specs/guides/TYPE_SYSTEM_GUIDE.md` step 3 (the
  optional method). History rows, `reviewed:`.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the five source files above; three documents |
| Existing tests | none change: without the attribute every read is as at HEAD (AC-2, AC-5) |
| Compatibility / data | No stored format changes. A record whose attribute is wrong now fails to load and is recomputed (Q3) |
| Feature matrix | the `RecordView` branch and constant are `records`-gated; `scripts/check-build-matrix.sh` rows `records`, `webui,records` |
| Performance | one map lookup per `RecordView` load; none for other types |
| Recovery | step 3 reverts alone; steps 1-2 are inert without an override |
| Certainty | High for the hook; medium for usefulness until the write side exists (Q1) |

## Phase 3: Examples and Tests

### Examples

```rust
let mut record = MetadataRecord::new();
record.with_type_identifier("RecordView".into()).with_status(Status::Source);
record.type_name = "record_view".into();
record.data_format = Some("csv".into());
record.set_attribute_from(RECORD_SCHEMA_ATTRIBUTE, &schema)?;   // metadata-application-attributes
store.set(&parse_key("data/t.csv")?, &csv_bytes, &Metadata::MetadataRecord(record)).await?;
let state = env.get_asset_manager().get_any_status(&parse_key("data/t.csv")?).await?;
assert_eq!(state.value()?.as_record_view()?.schema().as_ref(), &schema);
```

### Tests

Unit, `liquers-core/src/value.rs` `mod tests`:
- `default_deserialize_with_metadata_ignores_metadata` — core `Value` with and without metadata
  gives the same value: AC-5

Unit, `liquers-lib/src/value/extended.rs` `mod tests`:
- `combined_value_forwards_metadata_to_the_reading_half` — a test extension recording whether it
  received metadata, for a declared and an undeclared identifier: AC-6

Integration, new `liquers-lib/tests/records_schema_from_metadata.rs`,
`#![cfg(feature = "records")]`, `DefaultEnvironment<Value>` over an `AsyncMemoryStore`, entries
written with `store.set` (status `Source`):
- `csv_view_reads_with_schema_from_metadata` — the problem example for `csv`, `tsv`, `ndjson`,
  `json`, `md`; schema equal, roles kept: AC-1
- `view_without_attribute_is_inferred` — no attribute, and legacy metadata: `code` is `Int`: AC-2
- `ipc_view_ignores_metadata_schema` — `#[cfg(feature = "records-ipc")]`, attribute with a
  different field type: the embedded schema wins: AC-3
- `bad_schema_attribute_fails_the_read` — attribute `{"x": 1}`, and a schema declaring `code: Int`
  over `"abc"`: `get_any_status` returns `Err`: AC-4
- `fast_track_uses_schema_from_metadata` — a keyed recipe `data/t.csv` whose stored entry carries
  the attribute; `get` returns the declared schema without recomputing (command counter 0): AC-1

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test records_schema_from_metadata`,
`cargo test -p liquers-core --lib value::tests`, `cargo test -p liquers-lib --lib value::extended`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/value.rs` `DefaultValueSerializer::deserialize_from_bytes_with_metadata` and its test — `cargo test -p liquers-core --lib value::tests`. Rollback: revert the commit.
- [ ] 2. `liquers-core/src/assets.rs` `deserialize_stored_value` and its two callers — `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests` (no behaviour change). Depends on 1.
- [ ] 3. `liquers-lib/src/value/extended.rs` `CombinedValue` forwarding and its test — `cargo test -p liquers-lib --lib value::extended`. Depends on 1.
- [ ] 4. Requires `metadata-application-attributes` merged. `liquers-lib/src/records/mod.rs` constant; `liquers-lib/src/value/mod.rs` `read_record_view` and the `ExtValue` override; `liquers-lib/tests/records_schema_from_metadata.rs` — `cargo test -p liquers-lib --test records_schema_from_metadata`. Depends on 2, 3.
- [ ] 5. Documents (`VALUE_TYPE_SYSTEM.md`, `RECORD_STREAMS.md`, `TYPE_SYSTEM_GUIDE.md`; History rows, `reviewed:`); close `DESERIALIZATION-RECEIVES-NO-METADATA` with a resolution note — `python3 scripts/docs_index.py --check`.

### Validation

`CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features records --lib --tests`;
`bash scripts/check-build-matrix.sh`; `cargo check -p liquers-axum --tests`; diff review: only the
named files.
