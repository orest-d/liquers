# EGUI-VALUE-RENDERING

Status: Closed

## Summary
Complete egui rendering support for value variants that are currently unimplemented in conversion/render paths.

## Problem
`Metadata` and `CommandMetadata` value rendering in egui still contains `todo!()` paths, causing incomplete UI behavior and possible runtime panics when these variants are displayed.

## Goals
1. Render `Metadata` values in a structured, readable form.
2. Render `CommandMetadata` values in a structured, readable form.
3. Provide stable fallback rendering for unknown/partial substructures.

## Proposed Scope
1. Implement value-to-egui widgets for `Metadata` and `CommandMetadata`.
2. Add compact rendering helper for `CommandMetadata`.
3. Keep rendering deterministic and non-panicking.

## Current Code Anchors
1. `todo!()` for metadata rendering in `liquers-lib/src/egui/mod.rs`:
   1. `SimpleValue::Metadata`
   2. `SimpleValue::CommandMetadata`
2. Existing rendering helpers already live in `liquers-lib/src/egui/widgets.rs` (`display_asset_info`, `display_recipe`, `display_error`, etc.), so metadata/command-metadata renderers should follow the same placement/style.

## Rendering Design

### Metadata Rendering Functions
Add these functions in `liquers-lib/src/egui/widgets.rs`:

```rust
pub fn display_metadata(
    ui: &mut egui::Ui,
    metadata: &liquers_core::metadata::Metadata,
) -> egui::Response;
```

Expanded layout (recommended sections):
1. Header:
   1. status badge,
   2. title/filename,
   3. updated timestamp.
2. Identity:
   1. key,
   2. query,
   3. type identifier / type name.
3. Format:
   1. data format,
   2. media type,
   3. file size.
4. Progress:
   1. current progress entry/percentage,
   2. completion marker.
5. Messages and warnings:
   1. warning/error text blocks with distinct style.
6. Log preview:
   1. last N entries in compact list,
   2. optional expandable full log.

Compact layout:
1. status icon + title/filename,
2. one-line `type_identifier`, `data_format`, `media_type`,
3. short message snippet if present.

### CommandMetadata Rendering Functions
Add these functions in `liquers-lib/src/egui/widgets.rs`:

```rust
pub fn display_command_metadata(
    ui: &mut egui::Ui,
    command_metadata: &liquers_core::command_metadata::CommandMetadata,
) -> egui::Response;

pub fn display_command_metadata_compact(
    ui: &mut egui::Ui,
    command_metadata: &liquers_core::command_metadata::CommandMetadata,
) -> egui::Response;
```

`display_command_metadata` (full layout) should render:
1. Header:
   1. command name,
   2. namespace,
   3. doc/description.
2. Signature summary:
   1. state arg presence,
   2. argument count,
   3. return type (if present).
3. Arguments table:
   1. name,
   2. type,
   3. required/default,
   4. multiple/injected/context flags,
   5. hint string.
4. Enum arguments:
   1. alternatives list,
   2. `other` support marker where applicable.
5. Presets (if present):
   1. preset name,
   2. resolved values/links summary.

`display_command_metadata_compact` should render:
1. `namespace::name(arg1, arg2, ...)`,
2. short doc line,
3. badge with argument count and enum count.

### Safe Fallback Functions
For unknown or partial internals, add fallback helpers:
```rust
fn display_unavailable_field(ui: &mut egui::Ui, field: &str);
fn display_json_fallback(ui: &mut egui::Ui, value: &serde_json::Value);
```

Behavior:
1. never panic on missing fields,
2. show placeholder text `<not available>` where data is absent,
3. if complex nested structures cannot be rendered specifically, show JSON fallback.

## Integration into `UIValueExtension::show`
In `liquers-lib/src/egui/mod.rs`:
1. replace:
   1. `SimpleValue::Metadata { .. } => todo!()`
   2. `SimpleValue::CommandMetadata { .. } => todo!()`
2. with:
   1. `display_metadata(ui, value)`
   2. `display_command_metadata(ui, value)`

## Examples

### Example: Metadata value display
```rust
if let SimpleValue::Metadata { value } = data {
    display_metadata(ui, value);
}
```

### Example: CommandMetadata value display
```rust
if let SimpleValue::CommandMetadata { value } = data {
    display_command_metadata(ui, value);
}
```

### Example: Compact list of command metadata
```rust
for cmd in commands {
    display_command_metadata_compact(ui, cmd);
}
```

## Test Design Addendum
1. Unit tests for render helpers:
   1. metadata compact/expanded do not panic,
   2. command metadata compact/expanded do not panic.
2. Data-shape tests:
   1. metadata with missing optional fields,
   2. command metadata with enum args and injected args.
3. Regression tests:
   1. `UIValueExtension::show` on `SimpleValue::Metadata` no longer panics,
   2. `UIValueExtension::show` on `SimpleValue::CommandMetadata` no longer panics.

## Acceptance Criteria
1. No `todo!()` remains for these variants in egui rendering path.
2. Rendering works for representative metadata samples.
3. Tests cover both value variants and fallback behavior.
