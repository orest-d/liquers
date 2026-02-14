# Phase 1: High-Level Design - QueryConsoleElement

## Feature Name

QueryConsoleElement

## Purpose

A browser-inspired interactive query widget for exploring Liquers data. It provides an editable query bar with syntax highlighting, back/forward history navigation, value/metadata view toggle, and command preset suggestions. Unlike existing widgets that display a fixed query result, the QueryConsoleElement lets users iteratively compose, edit, and explore queries with full undo/redo and progressive result feedback.

## Core Interactions

### Query System
Parses user-entered query strings via `TryToQuery`. Appends preset actions to the last `TransformQuerySegment.query` when a "next" preset is selected. Uses `PlanBuilder` to resolve the last action's `CommandMetadata` and extract its `next: Vec<CommandPreset>` for the preset dropdown.

### Store System
No direct store interaction. Asset evaluation may read from stores as part of normal query execution.

### Command System
Adds `lui/query_console` command to create a QueryConsoleElement. Accepts a query, key, or string value as the initial query text. Uses existing `lui` namespace. No new command namespaces.

### Asset System
Introduces a new interaction pattern: **request_asset**. Unlike `submit_query` (which uses `evaluate_immediately` with payload), `request_asset` uses `evaluate` (async, no payload) and sends the resulting `AssetRef` back to the requesting widget via `UIElement::update`. The widget then monitors `AssetNotificationMessage` notifications for progress, value availability, and errors. No handle is involved; `evaluate` returns an `AssetRef` immediately and the `AssetManager` handles caching. The AppRunner's `evaluating` map is not used for this.

### Value Types
No new `ExtValue` variants. The widget stores and exposes the evaluated `Value` via `get_value()` / `get_metadata()`.

### Web/API
Not applicable.

### UI
Adds `QueryConsoleElement` widget in `liquers-lib/src/ui/widgets/`. Layout: top toolbar (back/forward buttons, query field with syntax highlighting via `edit_query()`, next-preset dropdown, value/metadata toggle), main content area (data view or scrollable metadata pane). Metadata pane layout (top to bottom): status (colored) with filename and title, description, error/message if any, progress indicator(s) if any, log, remaining metadata fields. Keyboard shortcuts: Enter to submit, Ctrl+Z/Ctrl+Y for history. No tabs or menus on the widget itself. History is persistent (serialized).

## Crate Placement

**liquers-lib** - Primary implementation
- `liquers-lib/src/ui/widgets/query_console_element.rs` - widget implementation
- `liquers-lib/src/ui/runner.rs` - AppRunner handles RequestAsset messages, calls evaluate, delivers AssetRef via update
- `liquers-lib/src/ui/message.rs` - new `AppMessage::RequestAsset` variant
- `liquers-lib/src/ui/commands.rs` - register `lui/query_console` command

No changes to liquers-core (AssetRef and notification infrastructure already exist).

## Resolved Design Decisions

1. **History**: Persistent across sessions (serialized with the element).
2. **Metadata view**: Scrollable pane (no tabs). Order: status (colored) + filename + title, description, error/message, progress indicators, log, remaining fields.
3. **Preset selection**: Auto-executes immediately (appends action to query and submits).
4. **Asset tracking**: No evaluating map involvement. `evaluate` returns `AssetRef` directly; AssetManager handles caching. AssetRef delivered to widget via `update`.

## References

- `specs/UI_INTERFACE_PHASE1_FSD.md` - UIElement trait and AppState design
- `liquers-lib/src/ui/element.rs` - existing UIElement implementations (AssetViewElement, StateViewElement)
- `liquers-lib/src/egui/widgets.rs` - `edit_query()` syntax highlighting function
- `liquers-core/src/command_metadata.rs` - `CommandPreset` and `CommandMetadata.next` field
- `liquers-core/src/assets.rs` - `AssetRef`, `AssetNotificationMessage`, notification channel
- `liquers-lib/src/ui/widgets/ui_spec_element.rs` - existing complex widget for pattern reference
