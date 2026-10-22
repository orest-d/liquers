---
id: METADATA-APPLICATION-ATTRIBUTES
kind: design
title: An open attribute map in MetadataRecord for application-defined facts
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/value, core/store, core/context]
issues: [CORE-METADATA-NO-APPLICATION-ATTRIBUTES]
created: 2026-10-19
---
# An open attribute map in MetadataRecord for application-defined facts

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented.

## Phase 1: High-Level Design

### Purpose

Let an application attach its own facts (tags, a source URL, a confidence score, a schema) to a
stored value's metadata, so they travel with the value through every store, listing and HTTP
metadata endpoint without Liquers understanding them. Core stores JSON; the crate that owns a
fact's type reads it back with `serde`.

### Problem Example

```rust
let mut record = MetadataRecord::new();
record.title = "Meeting notes".into();
// wanted: tags ["memory", "project-x"], source "https://example.org/notes/42"
store.set(&parse_key("memory/notes-42.md")?, body, &Metadata::MetadataRecord(record)).await?;
```

Today: `MetadataRecord` (`liquers-core/src/metadata.rs`) is a closed struct with
`#[serde(default, deny_unknown_fields)]`. There is no field to write tags into; a JSON document
with an extra `tags` key deserializes as `Metadata::LegacyMetadata`, which loses every typed
accessor. `AGENT-MEMORY-SERVICE` keeps tags in the Markdown front-matter and re-parses it on every
read, which does not work for a binary value.
Should be: `record.set_attribute("tags", json!(["memory", "project-x"]))`, and
`store.get_metadata(key)` and `listdir_asset_info` return the same attributes, with the version
unchanged by them.

### Scope and Acceptance Criteria

- **AC-1** Attributes round-trip through a store
  WHEN a record with attributes is written with `set` on the memory store, the file store, and a
  mount with a `metadata: { storage: per_entry }` section, and read back with `get_metadata`
  THEN the attributes are equal to those written, and the record is still a `MetadataRecord`
- **AC-2** Listings carry them
  WHEN `listdir_asset_info` lists a directory holding that key
  THEN its `AssetInfo` carries the same attributes
- **AC-3** Attributes are not content
  WHEN only the attributes of a stored value change (`set_metadata`), or a command sets one
  THEN the record's `version` is unchanged, and no dependent is expired
- **AC-4** A command can set one
  WHEN a command calls `context.set_attribute("confidence", json!(0.9))` for a keyed asset
  THEN the asset's metadata and its stored record carry it
- **AC-5** Typed at the edges
  WHEN `attribute_as::<T>(name)` reads an attribute that deserializes as `T`, one that does not,
  and one that is absent
  THEN it returns `Ok(Some(t))`, a `ConversionError`, and `Ok(None)`
- **AC-6** Nothing changes for records without attributes
  WHEN a record with an empty attribute map is serialized
  THEN the JSON has no `attributes` key, byte-identical to HEAD
- **AC-7** Templates and store handlers can read them
  WHEN a `MetadataRecord` is read as a `DataNode` with `get("attributes")`
  THEN it yields a map whose `tags` entry is the stored list
- **AC-8** Legacy metadata refuses writes
  WHEN `set_attribute` is called on `Metadata::LegacyMetadata`
  THEN it returns a `NotSupported` error, and `attribute` returns `None`

Non-goals: attributes declared in a recipe; handing metadata to `deserialize_from_bytes` (needed
before a stored table's schema can be read from metadata; filed as
`DESERIALIZATION-RECEIVES-NO-METADATA`); writing attributes from a `store_config` handler (waits on
`STORE-HANDLER-FIELD-ACCESS-HARDCODED`); Python accessors; search or indexing over attributes.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` field to `MetadataRecord` and `AssetInfo` and
  new `pub` methods, and changes their serialized form (rule 3 and rule 4).
- **Leading issue:** **Open design - recomputation:** whether a recomputed value inherits the
  attributes of the record it replaces.
- **Explanation:** a contained `liquers-core` change; every store serializes the whole record with
  serde, `finalize_metadata` touches named fields only, and `Version::from_content` hashes data
  bytes only, so preservation and version-independence already hold by construction and need tests,
  not mechanisms. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - recomputation:** a computed keyed asset is rewritten whole when it is
     recomputed. Should the new record inherit the old one's attributes? Recommended: no. Attributes
     describe the record they were written with; a recomputed value gets the attributes its command
     sets. Carrying them over would need a store read on every persist and would keep tags that no
     longer apply. Values written by an application (`Source`, `Override`) are not recomputed, so the
     memory-service case is unaffected.
  2. **Proposed resolution - shape:** one flat `BTreeMap<String, serde_json::Value>`; Liquers
     reserves no name and never writes into it. Documented convention for shared stores:
     `<application>.<name>` (`memory.tags`). Namespacing in the type would add structure no current
     user needs.
  3. **Proposed resolution - `AssetInfo`:** carries the attributes too (AC-2), so listings and the
     HTTP listing endpoints show tags without a per-key metadata read. Cost: a large attribute (a
     schema) enlarges listings; the remedy is to keep such facts small or in a separate key.
  4. **Proposed resolution - HTTP promise:** the metadata endpoints return the record as stored, so
     `attributes` appears as written; nothing else is promised.

### Design Dependencies

- `STORE-HANDLER-FIELD-ACCESS-HARDCODED` (issue, design `liquers-mustache`) — **overlaps**, weak:
  once handlers read through `DataNode`, a store spec can name `attributes.<x>`; AC-7 makes that
  possible. No order.
- `record-streams` — origin of the schema use (complete, E4): a stored table's schema is one
  consumer, which also needs `DESERIALIZATION-RECEIVES-NO-METADATA`.
- `SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` (issue) — **overlaps**, weak: a larger record
  costs more per clone. No order.

### Consolidated Findings

- `deny_unknown_fields` stays: unknown top-level keys still send a document to the legacy branch,
  which exists to preserve them. `attributes` is a known field, so a record with it stays typed.
- Every storage model and backend encodes the whole record with serde (`store/storage.rs`
  `encode_metadata`, the OpenDAL and file stores), so no store code changes.
- `MetadataRecord`'s `DataNode` impl (`liquers-core/src/data_node.rs`) answers unlisted names from
  the record's JSON, so `get("attributes")` needs no code; AC-7 pins it.
- The remaining load-path gap (metadata not reaching `deserialize_from_bytes`) is filed, not
  designed here.

## Phase 2: Architecture

### Solution

```rust
// MetadataRecord and AssetInfo, new field
#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
pub attributes: BTreeMap<String, serde_json::Value>,

// impl MetadataRecord
pub fn attribute(&self, name: &str) -> Option<&serde_json::Value>;
pub fn set_attribute(&mut self, name: impl Into<String>, value: serde_json::Value) -> &mut Self;
pub fn remove_attribute(&mut self, name: &str) -> Option<serde_json::Value>;
pub fn attribute_as<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Error>;
pub fn set_attribute_from<T: serde::Serialize>(&mut self, name: impl Into<String>, value: &T)
    -> Result<&mut Self, Error>;

// impl Metadata — record: delegate; legacy: read None, write Error::not_supported
pub fn attribute(&self, name: &str) -> Option<&serde_json::Value>;
pub fn set_attribute(&mut self, name: impl Into<String>, value: serde_json::Value)
    -> Result<&mut Self, Error>;

// impl Context
pub async fn set_attribute(&self, name: &str, value: serde_json::Value) -> Result<(), Error>;
```

- `attribute_as` maps a serde failure to `Error::conversion_error`; `set_attribute_from` maps a
  serialization failure to `Error::from_error(ErrorType::SerializationError, …)`.
- `MetadataRecord::get_asset_info` and `From<AssetInfo> for MetadataRecord` copy the map; the
  legacy `Metadata::get_asset_info` leaves it empty.
- `Context::set_attribute` writes through a new `pub(crate) AssetRef::set_attribute_from_command`
  that sets it on `lock.metadata` under the asset's write lock, as
  `set_description_fields_from_command` does for the title; it is persisted with the metadata like
  the title and never touches `version` (AC-3, AC-4).
- No store, pipeline or handler code changes; `finalize_metadata` / `finalize_metadata_empty`
  (`liquers-core/src/store.rs`) already leave unnamed fields alone.

Rejected: `Metadata::LegacyMetadata` as the carrier (loses typed fields); a `#[serde(flatten)]`
catch-all map (conflicts with `deny_unknown_fields` and lets a misspelt Liquers field become an
attribute silently); typed attributes in core (the schema type is `liquers-records`', not core's);
inheriting attributes on recomputation (Q1).

Known-issue preflight: `STORE-HANDLER-FIELD-ACCESS-HARDCODED` and
`SHAREABLE-METADATA-AND-BYTES-ARE-DEEP-COPIED` overlap weakly; `PY-VALUE-SERIALIZER-IS-A-STUB` is
unrelated (`liquers-py` wraps `MetadataRecord` opaquely and keeps compiling). Nothing open blocks.

### Changes

- `liquers-core/src/metadata.rs`: field on `MetadataRecord` and `AssetInfo`; the accessors above;
  `get_asset_info`, `From<AssetInfo>`.
- `liquers-core/src/assets.rs`: `AssetRef::set_attribute_from_command` (`pub(crate)`).
- `liquers-core/src/context.rs`: `Context::set_attribute`.
- No command, registry, HTTP route or binding change. Struct literals of `MetadataRecord` /
  `AssetInfo` elsewhere use `..Default::default()` or are in core (`liquers-axum`
  `value_description.rs` and its tests use `..Default::default()`; checked at HEAD).
- Documents: `specs/reference/ASSETS.md` (metadata fields: attributes, recomputation rule; History
  row, `reviewed:`), `specs/reference/STORE_SEMANTICS.md` (round-trip guarantee; History row,
  `reviewed:`), `specs/reference/WEB_API_SPECIFICATION.md` (metadata and listing JSON carry
  `attributes`; History row, `reviewed:`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A store drops the field | all encode the whole record; AC-1 on three stores | high |
| Existing stored records change | `skip_serializing_if` empty; AC-6 | high |
| A literal of `AssetInfo` / `MetadataRecord` outside core stops compiling | `Default` is derived; the listed literals use `..Default::default()`; `cargo test -p liquers-lib --lib --tests`, `liquers-axum`, `liquers-py` check | high |
| Large attributes slow listings | documented (Q3); no size limit imposed | medium |

Recovery: one commit per step; removing the field reverts every observable change.

## Phase 3: Examples and Tests

### Examples

```rust
record.set_attribute("memory.tags", json!(["memory", "project-x"]));
record.set_attribute_from("records.schema", &schema)?;          // liquers-records owns the type
let schema: Option<RecordSchema> = record.attribute_as("records.schema")?;
context.set_attribute("confidence", json!(0.9)).await?;         // inside a command
```

### Tests

Unit, `liquers-core/src/metadata.rs` `mod tests`:
- `attributes_absent_from_json_when_empty` — `serde_json::to_string` of `MetadataRecord::new()` unchanged: AC-6
- `attribute_as_reads_typed_reports_mismatch_and_absence`: AC-5
- `legacy_metadata_refuses_set_attribute`: AC-8
- `asset_info_carries_attributes_both_ways` — `get_asset_info`, `From<AssetInfo>`: AC-2

Unit, `liquers-core/src/data_node.rs` `mod tests`:
- `metadata_record_attributes_as_data_node` — `get("attributes")`, then `tags`: AC-7

New `liquers-core/tests/metadata_attributes.rs`:
- `attributes01_round_trip_memory_store` — `AsyncMemoryStore`: AC-1
- `attributes02_round_trip_file_store` — `AsyncFileStore` under a unique `temp_dir` path, as in `metadata_only_entry_reload.rs`: AC-1
- `attributes03_round_trip_per_entry_metadata_section` — store built from YAML with `metadata: { storage: per_entry }`, as in `store_adapters.rs`: AC-1
- `attributes04_listdir_asset_info_carries_attributes`: AC-2
- `attributes05_set_metadata_keeps_version_and_dependents` — keyed recipe chain as in `keyed_version_cascade.rs`; retag the root with `set_metadata`; root `version` equal, dependent still `Ready`: AC-3
- `attributes06_command_sets_attribute` — recipe `tagged/x.txt` ← `tag_me`, a command that calls `context.set_attribute`; live and stored metadata carry it; the version equals that of `untagged/x.txt` ← `plain`, the same value without the call: AC-3, AC-4

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test metadata_attributes` and
`cargo test -p liquers-core --lib metadata:: data_node::`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/metadata.rs` — `attributes` on `MetadataRecord` and `AssetInfo`, accessors on both types, `get_asset_info`, `From<AssetInfo>`; the four unit tests — `cargo test -p liquers-core --lib metadata::`. Rollback: revert the commit.
- [ ] 2. `liquers-core/src/data_node.rs` — `metadata_record_attributes_as_data_node` (no code change expected) — `cargo test -p liquers-core --lib data_node::`.
- [ ] 3. `liquers-core/src/assets.rs` `AssetRef::set_attribute_from_command`; `liquers-core/src/context.rs` `Context::set_attribute` — compiles; exercised in step 4.
- [ ] 4. `liquers-core/tests/metadata_attributes.rs` — the six tests — `cargo test -p liquers-core --test metadata_attributes`.
- [ ] 5. Dependent crates compile and pass — `cargo test -p liquers-lib --lib --tests`; `cargo check -p liquers-axum --tests`; `PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1 cargo check -p liquers-py` where the Python toolchain allows.
- [ ] 6. Docs: `ASSETS.md`, `STORE_SEMANTICS.md`, `WEB_API_SPECIFICATION.md` (History rows, `reviewed:`); close `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` with a resolution note naming `DESERIALIZATION-RECEIVES-NO-METADATA` as the remaining schema step — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (wasm rows: `liquers-web` reads metadata as JSON);
`python3 scripts/docs_index.py --check`; diff review for edits outside `metadata.rs`, `assets.rs`,
`context.rs`, the new test file and the named documents.
