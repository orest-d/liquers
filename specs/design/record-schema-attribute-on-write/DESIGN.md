---
id: RECORD-SCHEMA-ATTRIBUTE-ON-WRITE
kind: design
title: Serializing a record view to a schema-less format records its schema as a metadata attribute
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/assets, lib/value]
issues: [RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE]
created: 2026-10-26
---
# Serializing a record view to a schema-less format records its schema as a metadata attribute

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented.

## Phase 1: High-Level Design

### Purpose

A computed table stored as CSV must reload with the schema it was computed with. The read side
(`deserialization-with-metadata`) honours a `records.schema` attribute in the stored metadata, but
nothing writes it. Give a value a way to contribute attributes to the metadata written beside its
bytes, and let a `RecordView` written in a schema-less format contribute its schema.

### Problem Example

A keyed recipe `data/t.csv` produces a record view with schema `id: Int (Id role)`, `code: Text`,
`amount: Float`, rows `(1, "123", 2.0)`, `(2, "456", 3.0)`; the asset is persisted as `csv`.

Today (`liquers-core/src/assets.rs` persistence → `State::as_bytes` →
`DefaultValueSerializer::as_bytes`, which returns bytes only): the stored record carries no schema.
After a restart the fast track reads the CSV with `ReadSchema::Infer`: `code` is `Int`, `amount`
is `Int`, `id` has lost its role — the cached value disagrees with a fresh evaluation.
Should be: the stored record carries `attributes["records.schema"]` equal to the view's schema, and
(with `deserialization-with-metadata`) the reload has the computed schema.

### Scope and Acceptance Criteria

- **AC-1** A schema-less write records the schema
  WHEN a record view is persisted by the asset layer as `csv`, `tsv`, `ndjson`, `jsonl`, `json` or
  `md`
  THEN the stored metadata and the asset's live metadata carry `records.schema` equal to the
  view's `RecordSchema` (serde form), roles and nullability included
- **AC-2** Self-describing formats add nothing
  WHEN the same view is persisted as `ipc` or `parquet`
  THEN no `records.schema` attribute is written (the file embeds the schema)
- **AC-3** Other values add nothing
  WHEN a DataFrame, an image, a base value or a foreign value is persisted
  THEN no attribute is added, and `serialization_attributes` of a type that does not override it,
  and of `CombinedValue` holding such a value, is empty
- **AC-4** Every serializing path records it
  WHEN a record view reaches the store through `set_state` (both managers) or is serialized by
  `get_binary` with no cached binary
  THEN the stored and the returned metadata carry the attribute, as for persistence
- **AC-5** The attribute is not content
  WHEN the attribute is written
  THEN the asset's `version` is the content hash of the bytes alone, unchanged from HEAD
- **AC-6** End to end
  WHEN the problem example is evaluated, the environment is rebuilt over the same store, and the
  key is read again (with `deserialization-with-metadata` in place)
  THEN the value is served from the store and its schema equals the computed one

Non-goals: the read side (`deserialization-with-metadata`); a schema for `RecordSource` (its byte
form is a manifest); writing the attribute from `set_binary` (no value in hand); a `polars.DataFrame`
dtype attribute (possible later through the same hook).

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` default method to `DefaultValueSerializer`
  (rule 4) and spans `liquers-core` and `liquers-lib` (rule 6); requires two unimplemented designs.
- **Leading issue:** **Open design - hook shape:** a separate `serialization_attributes` method
  beside `serialization_warnings`, or one combined "serialization facts" method.
- **Explanation:** the five asset-layer sites that already hold the metadata while serializing are
  the ones `serialization-lossy-write-warnings` edits; the attribute map comes from
  `metadata-application-attributes`. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - hook shape:** recommended a separate defaulted method,
     `fn serialization_attributes(&self, data_format: &str) -> BTreeMap<String, serde_json::Value>`,
     called at the same sites as `serialization_warnings`. Both are defaulted and independent, so
     either design can land first; a combined `serialization_facts` struct would force the
     not-yet-implemented warnings design to change its signature.
  2. **Proposed resolution - overwrite:** the serializer's attribute replaces one of the same name
     already on the record (for example set by a command with `context.set_attribute`): it
     describes the bytes being written, and a disagreeing hand-set schema would make the
     declared-schema read fail.
  3. **Proposed resolution - which formats:** exactly the `TableFormat`s without an embedded schema
     (`Csv`, `NdJson`, `Json`, `Markdown`, `Html`); `Ipc` and `Parquet` add nothing (AC-2), which
     also keeps listings small (`metadata-application-attributes` Q3 notes attributes travel in
     `AssetInfo`).

### Design Dependencies

- `metadata-application-attributes` — **requires**: `MetadataRecord::attributes` and
  `set_attribute`. Nothing here can land before it.
- `deserialization-with-metadata` — **required-by** (its leading question names this issue as the
  producer) and **requires** its `RECORD_SCHEMA_ATTRIBUTE` constant (T2, shared attribute
  contract). Whichever lands first defines the constant in `liquers-lib/src/records/mod.rs`; AC-6
  needs both.
- `serialization-lossy-write-warnings` — **overlaps** (T3, the same five call sites in
  `assets.rs` and the same trait). Not attached: its Phase 4 is finished. No functional order; the
  second to land adds its call beside the first's, reusing its "serialize once in `set_state`" change
  if present.
- `VALUE-SERIALIZATION-IS-SYNCHRONOUS-AND-WHOLE-VALUE` (issue) — **overlaps**, weak: a streamed
  writer should keep this hook for in-memory values.

### Consolidated Findings

- The attribute is computed from the value, not from the bytes, so it costs one `serde_json`
  conversion of a schema per write of a record view; no serialization is repeated.
- `liquers-records`' `RecordSchema` already derives `Serialize`/`Deserialize` (through
  `RecordSchemaSpec`), the form the read side parses.
- Attributes do not enter `Version::from_content` (`metadata-application-attributes` AC-3), so
  AC-5 holds by construction and needs a test, not a mechanism.

## Phase 2: Architecture

### Solution

1. `liquers-core/src/value.rs`, `DefaultValueSerializer`:

   ```rust
   /// Attributes to record in the metadata written with `as_bytes(data_format)`'s bytes. Must not
   /// fail and must not serialize. The default adds none.
   fn serialization_attributes(&self, data_format: &str) -> BTreeMap<String, serde_json::Value> {
       let _ = data_format;
       BTreeMap::new()
   }
   ```

2. `liquers-lib/src/value/extended.rs`, `CombinedValue<B, E>`: forwards to the half it holds.
3. `liquers-lib/src/value/mod.rs`, `ExtValue` overrides: `RecordView { value }` →
   `TableFormat::from_data_format(format)`; for `Csv | NdJson | Json | Markdown | Html` and when
   `serde_json::to_value(value.schema())` succeeds, one entry `RECORD_SCHEMA_ATTRIBUTE`; `Ipc |
   Parquet`, an unknown format, or a conversion failure → empty. Every other variant → empty.
   Explicit arms (`#[cfg]` per variant, as the neighbouring methods).
4. `liquers-core/src/assets.rs`: private
   `fn add_serialization_attributes(metadata: &mut Metadata, attributes: BTreeMap<String, serde_json::Value>)`
   — `set_attribute` per entry on a `MetadataRecord`; nothing on legacy metadata (no `_ =>`). Called
   where the value and the effective data format are in hand: `prepare_version`
   (`PreparedVersion.attributes`, applied in `finalize_status_with_version` before
   `save_to_store`), `serialize_to_binary`, `get_binary_any_status`, and both `set_state` bodies.
5. `liquers-lib/src/records/mod.rs`: `pub const RECORD_SCHEMA_ATTRIBUTE: &str = "records.schema";`
   unless `deserialization-with-metadata` already added it.

Rejected: the `ns-rec` commands set the attribute themselves (misses every other producer of a
view, and a command does not know the format its result will be written in); a combined facts
method (Q1); extending `as_bytes` to return attributes (breaks every implementor).

### Changes

- `liquers-core/src/value.rs` (trait method), `liquers-core/src/assets.rs` (helper, five sites,
  `PreparedVersion`), `liquers-lib/src/value/extended.rs`, `liquers-lib/src/value/mod.rs`,
  `liquers-lib/src/records/mod.rs`.
- No command, registry, HTTP or binding change. Sync throughout; each site already holds the lock
  or owns the metadata.
- Documents: `specs/reference/VALUE_TYPE_SYSTEM.md` §Writing (values may contribute attributes),
  `specs/reference/RECORD_STREAMS.md` (stored views carry `records.schema` for text formats),
  `specs/guides/TYPE_SYSTEM_GUIDE.md` (the optional method). History rows, `reviewed:`.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the five source files; three documents |
| Existing tests | none: no type overrides the method except `ExtValue`, and no test inspects attributes today |
| Compatibility / data | stored records of text-format views gain an `attributes` key; readers before `deserialization-with-metadata` ignore it |
| Feature matrix | override and constant `records`-gated; `bash scripts/check-build-matrix.sh` |
| Performance | one schema-to-JSON conversion per record-view write; listings carry the schema (Q3) |
| Recovery | step 5 (override) reverts alone; the hook is inert without it |
| Certainty | High for the mechanism; ordering depends on two unimplemented designs |

## Phase 3: Examples and Tests

### Examples

```rust
let view: Arc<dyn RecordView> = /* the problem example's batch */;
let v = Value::from_record_view(view.clone());
let attrs = v.serialization_attributes("csv");
assert_eq!(attrs[RECORD_SCHEMA_ATTRIBUTE], serde_json::to_value(view.schema())?);
assert!(v.serialization_attributes("parquet").is_empty());
```

### Tests

Unit, `liquers-core/src/value.rs` `mod tests`:
- `default_serialization_attributes_are_empty` — core `Value`, several formats: AC-3

Unit, `liquers-lib/src/value/extended.rs` `mod tests`:
- `combined_value_forwards_serialization_attributes` — a test extension returning one entry; a base
  value returns none: AC-3

Unit, `liquers-lib/src/value/mod.rs` `mod tests` (`#[cfg(feature = "records")]`):
- `record_view_attributes_for_text_formats` — the six spellings carry the schema: AC-1
- `record_view_attributes_empty_for_ipc_and_parquet`: AC-2
- `non_record_values_add_no_attributes` — image; DataFrame under `polars`: AC-3

Integration, new `liquers-lib/tests/records_schema_on_write.rs`, `#![cfg(feature = "records")]`,
`DefaultEnvironment<Value>` over an `AsyncMemoryStore`, a registered command producing the view,
recipe `data/t.csv`:
- `persisted_csv_view_records_its_schema` — `get`, then `store.get_metadata`: AC-1
- `persisted_parquet_view_has_no_schema_attribute` — `#[cfg(feature = "records-parquet")]`: AC-2
- `set_state_records_schema_on_both_managers` — default and inline managers: AC-4
- `get_binary_returns_metadata_with_schema` — non-keyed query of the command: AC-4
- `schema_attribute_does_not_change_version` — version equals `Version::from_content(stored bytes)`: AC-5
- `reload_after_restart_keeps_schema` — rebuild the environment over the store; command counter
  unchanged, schema equal: AC-6 (lands with, or after, `deserialization-with-metadata`)

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test records_schema_on_write`,
`cargo test -p liquers-core --lib value::tests`, `cargo test -p liquers-lib --lib value::`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Precondition: `metadata-application-attributes` merged. `liquers-core/src/value.rs` — `serialization_attributes` default and its test — `cargo test -p liquers-core --lib value::tests`.
- [ ] 2. `liquers-lib/src/value/extended.rs` — `CombinedValue` forwards; test — `cargo test -p liquers-lib --lib value::extended`. Depends on 1.
- [ ] 3. `liquers-lib/src/records/mod.rs` constant (if absent); `liquers-lib/src/value/mod.rs` `ExtValue` override; unit tests — `cargo test -p liquers-lib --lib value::`. Depends on 2.
- [ ] 4. `liquers-lib/tests/records_schema_on_write.rs` — AC-1..AC-5 tests (fail at HEAD) — `cargo test -p liquers-lib --test records_schema_on_write`.
- [ ] 5. `liquers-core/src/assets.rs` — `add_serialization_attributes`, `PreparedVersion.attributes`, the five sites (beside `serialization_warnings` calls if present) — step 4's tests pass; `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib --tests`. Rollback: revert this step; the hook stays inert.
- [ ] 6. When `deserialization-with-metadata` is merged: `reload_after_restart_keeps_schema` — AC-6.
- [ ] 7. Documents (`VALUE_TYPE_SYSTEM.md`, `RECORD_STREAMS.md`, `TYPE_SYSTEM_GUIDE.md`; History rows, `reviewed:`); close `RECORD-SCHEMA-NOT-RECORDED-IN-METADATA-ON-WRITE` — `python3 scripts/docs_index.py --check`.

### Validation

`CARGO_INCREMENTAL=0 cargo test -p liquers-lib --lib --tests`;
`cargo test -p liquers-lib --no-default-features --features records --lib --tests`;
`bash scripts/check-build-matrix.sh`; `cargo check -p liquers-axum --tests`; diff review: only the
named files.
