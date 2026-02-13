# Phase 1: High-Level Design - menu-pane-layout

## Feature Name

Menu Bar and Configurable Pane Layout

## Purpose

Provide a complete application frame UIElement with menu bar (menus, buttons, shortcuts) and configurable pane layout, enabling users to build desktop-style applications with familiar menu-driven interfaces similar to traditional GUI applications (File/Edit/View menus, toolbars, keyboard shortcuts).

## Core Interactions

### Query System
Menu items and toolbar buttons trigger query submission via UIContext (same pattern as ui_button_app). Menu structure is declarative, not query-driven.

### Store System
No direct Store interaction. Serialization/deserialization of menu structure and layout configuration via AppState.

### Command System
New command in `lui` namespace: `ui_spec` - Creates UISpecElement from YAML specification passed via state (text or binary). Deserializes to declarative struct. Validates keyboard shortcuts for conflicts (reports via context warnings). Future: spec can be extended beyond menu/layout.

### Asset System
No direct Asset interaction. Menu actions trigger queries that may create/consume assets.

### Value Types
No new ExtValue variants. Menu structure uses plain Rust structs (serializable).

### Web/API (if applicable)
Not applicable. UI-only feature.

### UI
Adds **UISpecElement** - Combined UIElement with menu bar (menus, toolbar buttons, shortcuts) and configurable pane layout (horizontal/vertical splits, tabs). Created via `lui/ui_spec` command from YAML specification.

## Crate Placement

**liquers-lib** - New UI widgets module (`liquers-lib/src/ui/widgets/ui_spec_element.rs`)

Rationale: Complex widget warrants own module. Group similar complex widgets in `ui/widgets/` (vs. simple elements in `ui/elements/`). Builds on existing Phase 1 UI infrastructure (UIElement trait, UIContext, AppState). No core abstractions changed.

## Open Questions

**Resolved (see decisions above):**
1. ✅ Configuration format: YAML → deserialize to declarative struct
2. ✅ Resizable splits: Not initially (out of scope unless trivial in egui, then configurable property)
3. ✅ Shortcut conflicts: Detect during command execution, report via context warnings
4. ✅ Menu state: Out of scope (future: dynamic via queries)

**Remaining for Phase 2:**
1. YAML schema structure: top-level keys (menu, layout, other future properties)?
2. Layout types: exact split/tab/pane structure (nested vs flat)?
3. How to reference child UIElements in layout (by handle, by inline spec)?

## References

- `specs/UI_INTERFACE_PHASE1_FSD.md` - UIElement trait definition
- `specs/UI_INTERFACE_PHASE1b.md` - AppRunner pattern, message handling
- `liquers-lib/examples/ui_button_app.rs` - Query submission pattern for actions
