# Phase 2: Solution and Architecture

## Core (`liquers-core/src/type_system.rs`)

```rust
pub struct TypeInfo {
    // …
    /// Data formats this type can be written to. A `data_format` outside this set is what the
    /// write path refuses.
    pub supported_data_formats: Vec<Cow<'static, str>>,
    /// The subset of `supported_data_formats` that cannot be read back (presentation formats such
    /// as an HTML table). A stored value in one of these is recomputed, never deserialized.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_only_data_formats: Vec<Cow<'static, str>>,
}

impl TypeInfo {
    /// Declare `data_format` writable but not readable. Also adds it to the supported formats.
    pub fn with_write_only_data_format(self, data_format: impl Into<Cow<'static, str>>) -> Self;
    pub fn can_read_data_format(&self, data_format: &str) -> bool;
}

impl TypeRegistry {
    pub fn can_read_data_format(&self, type_identifier: &str, data_format: &str) -> bool;
}
```

`TypeInfo::new` initializes the field to empty. Check whether any struct literal of `TypeInfo`
exists outside the builder (the doc forbids it). Derive-based equality (`PartialEq`) includes the
new field.

## Fast track (`liquers-core/src/assets.rs`, `try_fast_track`)

Before `deserialize_stored_value`: if the registry knows `type_identifier` and
`!can_read_data_format(type_identifier, data_format)`, then `eprintln!` "stored as write-only
format X; recomputing", call `clear_fast_track_payload()`, and return `Ok(false)`. The recovery
reads (`get_any_status`) return a typed error, "stored in a write-only format", rather than
attempting to deserialize. Check `get_any_status`'s store path.

## liquers-lib

`liquers-lib/src/value/mod.rs`, `RecordView`'s `TypeInfo`: replace `.with_data_format("html")` (or
its equivalent in the list) with `.with_write_only_data_format("html")`.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `DATA-FORMAT-CONSTANTS-AND-TOOLING` (P2, L) | Natural long-term home | No, unless the maintainer chooses to defer (question 2) |
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | Same function, sibling branch | No |

## Relevant commands

None changed. Writing `ns-rec` views as `.html` keeps working.

## Documentation architecture

VALUE_TYPE_SYSTEM.md field list and record identifiers. TYPE_SYSTEM_GUIDE step 4 sentence.
History rows, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `type_system.rs`, `assets.rs`, `liquers-lib/src/value/mod.rs`, docs |
| Serialization | Additive field, omitted when empty |
| Bindings | `liquers-py`/`liquers-web` read `supported_data_formats` only, so they are unaffected |
| Existing tests | `record_typeinfo.rs` may assert the `html` declaration. Update it. |
| Recovery | Revert |
| Certainty | High once decided |
