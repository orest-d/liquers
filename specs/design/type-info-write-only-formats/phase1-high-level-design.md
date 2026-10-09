# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a serialized `pub` field and methods to `TypeInfo`
  (rules 3-4)
- **Leading issue:** **Open design question — representation, and whether to settle it here or in
  `DATA-FORMAT-CONSTANTS-AND-TOOLING`** (P2, L, no design), which the issue names as the natural
  home. `TypeInfo` is serialized (`Serialize, Deserialize`), so a new field is a format addition.
- **Explanation:** A small additive representation works today and does not prevent the larger
  vocabulary work from absorbing it later. The design specifies it so it can proceed independently
  if the maintainer chooses.
- **Open questions:**
  1. **Proposed resolution — subset list.** Keep `supported_data_formats` meaning "can be written"
     (which is how the write path and `liquers-lib`/`liquers-py`/`liquers-web` already use it).
     Add `write_only_data_formats: Vec<Cow<'static, str>>`, a subset of it, with `#[serde(default,
     skip_serializing_if = "Vec::is_empty")]`, a builder `with_write_only_data_format`, and
     `TypeInfo::can_read_data_format(&self, f) -> bool`. Alternative: a per-format capability
     struct. Rejected as heavier, and it belongs to the vocabulary redesign.
  2. **Open design question — scope ownership:** do it now (recommended, because `RecordView`'s
     `html` is a live false claim), or defer to `DATA-FORMAT-CONSTANTS-AND-TOOLING`.

## Problem

`TypeInfo::supported_data_formats` (`liquers-core/src/type_system.rs`) is documented as formats a
type "can be written to and read from", and the write path refuses anything else. A deliberately
write-only format (record-streams' HTML table) must either be declared, which falsely claims it
can be loaded (the load fails and is logged as corrupted), or be left out, which makes it
unrequestable. `RecordView` declares `html` today.

## Expected behaviour and acceptance

1. `RecordView`'s `TypeInfo` declares `html` as write-only. Writing `data.html` still works.
2. Loading a stored `RecordView` whose data format is write-only skips deserialization, logs no
   corruption, and recomputes from the recipe (fast track returns `Ok(false)` quietly).
3. `TypeRegistry::can_read_data_format(type, format)` answers `false` for it, and `true` for the
   other declared formats.
4. Serialized `TypeInfo` without the new field deserializes, and with an empty list serializes
   unchanged.
5. The doc comment of `supported_data_formats` says "written". Read support is all of them minus
   `write_only_data_formats`.

## Scope

Core type system, the fast-track check, and `RecordView`'s declaration. Other presentation formats
can adopt it later.

## Design Dependencies

- `metadata-only-entry-reload` — **overlaps**. A sibling quiet-recompute branch in the same
  function. Implement in either order. Both add an early `Ok(false)`.
- `simple-value-serializer-parity` — **overlaps** (format declarations).

## Documentation assessment

- Reference: `specs/reference/VALUE_TYPE_SYSTEM.md` (the `TypeInfo` field list; record identifiers
  "html write-only"), plus History and `reviewed:`.
- Guide: `TYPE_SYSTEM_GUIDE.md` step 4: how to declare a write-only format.

## Consolidated Findings

- `liquers-core/src/value.rs:~409` already comments that the list means "written", so the doc
  comment on the field is the inconsistent part. Clarifying it is safe.
- The fast track knows `type_identifier` and `data_format` before deserializing, and has the
  registry (`registry_owner.get_type_registry()`). The check is one lookup.
- The registry might not know the type (degraded build). Keep the existing "not registered" path
  first.
