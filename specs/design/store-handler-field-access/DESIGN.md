---
id: STORE-HANDLER-FIELD-ACCESS
kind: design
title: Store handlers reach record fields through the record's DataNode implementation
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/store, core/value]
issues: [STORE-HANDLER-FIELD-ACCESS-HARDCODED]
created: 2026-10-20
---
# Store handlers reach record fields through the record's DataNode implementation

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. Continues the refactor that the completed `design/liquers-mustache/`
(`data-node-interface.md`) left to this issue: the read side (`DataNode for MetadataRecord`)
shipped there; the write side (`DataNodeMut`) did not.

## Phase 1: High-Level Design

### Purpose

Make `MetadataRecord`'s own `DataNode` implementation, plus a new `DataNodeMut` write side, the one
place that says which record fields a store spec can read, require and set, so the `set` and
`require` handlers (and `liquers-lib`'s `front_matter`) stop carrying a parallel six-name `match`,
and a field added to the record — application attributes first — becomes nameable in store
configuration without touching the handlers.

### Problem Example

```yaml
metadata:
  on_validate: require-title-filename
  on_write: defaults/set-type_name-note/set-attributes.source-import
```

Today: `require-title-filename` fails the build with "field 'filename' is not one of title,
description, media_type, data_format, type_identifier, unicode_icon"
(`handlers::check_text_field`, `liquers-core/src/store/handlers.rs`), although
`MetadataRecord::get("filename")` (`liquers-core/src/data_node.rs`) answers it; and once
`METADATA-APPLICATION-ATTRIBUTES` adds attributes, `set-attributes.source-import` would fail the
same way. Should be: both build; `require` refuses a record with an empty `filename`, and `set`
writes the attribute `source = "import"`.

### Scope and Acceptance Criteria

- **AC-1** `require` reads any record field
  WHEN `require-<f>` names a field `MetadataRecord`'s `DataNode` answers (`filename`, `key`,
  `type_name`, `file_size`, a text field)
  THEN the spec builds, and a write whose record has that field null, `""` or an empty list is
  refused naming it
- **AC-2** `set` writes what the record lets it
  WHEN `set-<f>-<v>` names a field `DataNodeMut for MetadataRecord` accepts
  THEN the field holds `v` after the step; `mode = fill` leaves a non-empty field alone
- **AC-3** Unknown and protected fields fail the build
  WHEN `set` names an unknown field or one the record does not let configuration set (`key`,
  `status`, `version`, `file_size`), or `require` names an unknown field
  THEN the build fails with a `configuration_error` listing the accepted fields
- **AC-4** Attributes are nameable
  WHEN `METADATA-APPLICATION-ATTRIBUTES` has landed and a spec says `set-attributes.<name>-<v>` or
  `require-attributes.<name>`
  THEN the attribute is written as a JSON string, or required to be present and non-empty
- **AC-5** Behaviour of the six text fields is unchanged
  WHEN the existing `handlers.rs`, `realm.rs`, `pipeline.rs` and `liquers-lib/src/store_config.rs`
  tests run
  THEN they pass unchanged, `front_matter` included
- **AC-6** One vocabulary
  WHEN a field is added to `MetadataRecord`'s `DataNodeMut` implementation
  THEN `set`, `require` and `front_matter` accept it with no edit in `handlers.rs` or
  `liquers-lib`

Non-goals: setting non-string values from a spec (numbers, lists); `front_matter` copying lists
into attributes; templates writing fields; renaming the `text_field` helpers.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` trait `DataNodeMut` and its implementation to
  `liquers-core` (rule 3) and widens the accepted configuration (rule 4).
- **Leading issue:** **Open design - settable fields:** which `MetadataRecord` fields a spec may
  set.
- **Explanation:** a `liquers-core` change behind unchanged helper signatures, so `liquers-lib`
  follows without edits. One vocabulary decision remains; Phases 3-4 assume the recommendation.
- **Open questions:**
  1. **Open design - settable fields:** the six text fields, `type_name`, `message` and
     `attributes.<name>` (recommended: descriptive fields only), or every serialized field
     (would let a spec forge `status`, `key` or `version`, which the store and asset manager
     trust).
  2. **Proposed resolution - path syntax:** one level of dotted path, `attributes.<name>`
     (recommended: `.` is a literal in a spec and never in a field name); no deeper nesting.
  3. **Proposed resolution - empty:** null, `""`, an empty list or an empty map is empty for
     `require` and for `mode = fill` (recommended: matches today's text rule and Mustache
     truthiness except that `false` and `0` are values).
  4. **Proposed resolution - order with attributes:** land `metadata-application-attributes` first
     (recommended: AC-4 is the user-visible gain); if this lands first, AC-4's step is done in
     that design instead.

### Design Dependencies

- `metadata-application-attributes` — **overlaps** (its design lists this issue as a weak
  overlap): AC-4 needs its `attributes` field; recommended order: it first (Q4). Not merged: E1
  does not apply, but each stands alone and T1-T4 do not hold.
- `liquers-mustache` (complete, E4) — origin of `DataNode for MetadataRecord`; not extended.
- `store-config-key-scoped-handlers` — **overlaps**, weak: both edit `store_config`, different
  functions.
- `template-non-scalar-text-form` — **overlaps**, weak: also extends `DataNode`; no shared method.

### Consolidated Findings

- `DataNode for MetadataRecord` already answers `filename`, `key`, `query`, `type_name`,
  `file_size`, `is_dir` and any serialized field through `snapshot_field`; reads need no new code.
- `liquers-lib/src/store_config.rs` uses `check_text_field`, `text_field` and `set_text_field` only;
  keeping their signatures keeps the change in one crate.
- `snapshot_field` serializes the whole record per unlisted name; the handlers resolve the names
  they need at build and read borrowed fields on the hot path, so the cost stays at build.
- `STORE_CONFIG_GUIDE.md` §5 ("The text fields are …", `set`, `require` rows) and §10
  (`check_text_field`) change.

## Phase 2: Architecture

### Solution

`liquers-core/src/data_node.rs`:

```rust
/// Writing fields of a map node: the vocabulary a configuration may set.
pub trait DataNodeMut: DataNode {
    /// Refuses a field this node does not let callers set, naming the ones it does.
    fn check_settable(&self, name: &str) -> Result<(), Error>;
    /// Sets field `name`; `Value::Null` clears an optional field. A refused name or a value
    /// of the wrong kind is an error.
    fn set_field(&mut self, name: &str, value: serde_json::Value) -> Result<(), Error>;
}

/// Field `path` (`name` or `name.sub`) of a node, walking `get`.
pub fn get_path<'a>(node: &'a dyn DataNode, path: &str) -> Option<NodeRef<'a>>;
/// Whether a node is empty for `require` and `fill` (Q3).
pub fn is_empty_node(node: Option<NodeRef<'_>>) -> bool;
```

`impl DataNodeMut for MetadataRecord`: an exhaustive `match` over the settable names of Q1 (each
arm the assignment `set_text_field` does today; `media_type`, `data_format` become `Some(text)` or
`None` for null), plus the `attributes.<name>` arm once that field exists; a non-string value for a
text field is `Error::conversion_error`. Unknown or protected names are `Error::not_supported`
naming the accepted list.

`liquers-core/src/store/handlers.rs`, same signatures:

- `check_text_field(field)` → `MetadataRecord::new().check_settable(field)` mapped to
  `configuration_error`.
- `text_field(record, field)` → `get_path(record, field)` as text (scalar text, `""` when empty).
- `set_text_field(record, field, value, mode)` → if `mode.applies_to(is_empty_node(get_path(…)))`,
  `record.set_field(field, Value::String(value))`; an error cannot occur for a checked field and is
  ignored as today's `_ => {}` arm is.
- `build_require`: checks each field with a new private `check_readable` (`get_path` on a fresh
  record, or the `attributes.` prefix), then refuses on `is_empty_node`.
- `TEXT_FIELDS` stays as the documented list of text fields; the messages list the accepted names
  from `check_settable`'s error.

Rejected: a second `match` in a `FIELDS` table beside the record (the issue's defect again);
`DataNodeMut::set(name, Scalar)` as the note sketched (cannot carry an attribute list later);
reflection through serde (would accept every field, Q1).

Known-issue preflight: none blocking. No command; `set` and `require` keep their argument lists,
so `specs/command_registry.yaml` is regenerated only for their new doc strings.

### Changes

- `liquers-core/src/data_node.rs`: `DataNodeMut`, `get_path`, `is_empty_node`, the impl.
- `liquers-core/src/store/handlers.rs`: the helper bodies above; `build_set`, `build_require` doc
  strings; `register_builtin_handlers` docs of `set` and `require`.
- Documents: `STORE_CONFIG_GUIDE.md` §5, §10; `TEMPLATE_LANGUAGE.md` only if it lists the
  `DataNode` traits (check at step 4); History rows, `reviewed:`.

Errors: `configuration_error` at build, `conversion_error` / `not_supported` from `set_field`. Sync
(`CompiledFn` is sync; `DataNode` for a record is sync).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| `text_field` on an optional field returned `""` for `None`; `get_path` yields `Null` | `is_empty_node` treats `Null` as empty; AC-5 tests | high |
| `require-file_size` on a directory record | a directory has no record write; unchanged | high |
| A protected field becomes settable by mistake | exhaustive `match`; AC-3 test lists the protected names | high |
| Attributes not yet merged | AC-4's step is skipped and moves to that design (Q4) | medium |

Recovery: revert; the helper signatures never changed.

## Phase 3: Examples and Tests

### Examples

Validated (`liquers-validate`, filled form): `-store_config/require-title-filename` and
`-store_config/defaults/set-type_name-note/set-attributes.source-import` both plan (the field
names are not checked by the validator, as `STORE_CONFIG_GUIDE.md` §7 says).

### Tests

`liquers-core/src/data_node.rs` `mod tests`:

- `metadata_record_sets_descriptive_fields` — each settable name; `media_type` null clears: AC-2
- `metadata_record_refuses_protected_fields` — `key`, `status`, `version`, `file_size`, `nope`: AC-3
- `get_path_and_emptiness` — `title`, `media_type` (null), `attributes.x` absent: AC-1

`liquers-core/src/store/handlers.rs` `mod tests`:

- `require_reads_any_record_field` — `require-filename`, `require-type_name`: AC-1
- `set_and_require_refuse_unknown_fields_at_build` — AC-3
- `set_fill_respects_non_empty_fields` — AC-2
- `attribute_fields_are_settable_and_required` — after attributes land: AC-4, AC-6

Unchanged suites as regression: `cargo test -p liquers-core --lib store::` and
`cargo test -p liquers-lib --lib store_config` (front matter): AC-5, AC-6.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/data_node.rs` — `DataNodeMut`, `get_path`, `is_empty_node`, `impl DataNodeMut for MetadataRecord` (Q1 names) and its tests — `cargo test -p liquers-core --lib data_node`. Rollback: revert (additive).
- [ ] 2. `liquers-core/src/store/handlers.rs` — helper bodies, `build_require` readable check, docs; tests above except attributes — `cargo test -p liquers-core --lib store::handlers`, then `cargo test -p liquers-lib --lib store_config`. Depends on 1. Rollback: restore the bodies.
- [ ] 3. Requires `metadata-application-attributes` implemented: the `attributes.<name>` arm in `set_field` / `check_settable`, `get("attributes")` answering an empty map, and `attribute_fields_are_settable_and_required` — `cargo test -p liquers-core --lib attribute_fields`. If that design is not implemented yet, leave this step unticked and note it there.
- [ ] 4. Regenerate `specs/command_registry.yaml` (doc strings) with a CHANGELOG line — `cargo test -p liquers-lib --test registry_export`.
- [ ] 5. `STORE_CONFIG_GUIDE.md` §5, §10; History, `reviewed:`; close `STORE-HANDLER-FIELD-ACCESS-HARDCODED` (or note step 3 outstanding) — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (wasm32: `DataNodeMut` has `MaybeSend + MaybeSync` through
`DataNode`); diff review: no edit in `liquers-lib`.
