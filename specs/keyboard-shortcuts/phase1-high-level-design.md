# Phase 1: High-Level Design - Keyboard Shortcuts

## Feature Name

Platform-Independent Keyboard Shortcuts Library

## Purpose

Provide a unified, reusable representation for keyboard shortcuts that handles platform differences (macOS ⌘ vs Windows/Linux Ctrl) with string parsing/serialization, serde support, and conversions to/from egui, ratatui, and dioxus/web UI frameworks. Replaces ad-hoc shortcut parsing currently embedded in `ui_spec_element.rs`.

## Core Interactions

### Query System
Not applicable - this is a utility library for UI components.

### Store System
Not applicable - shortcuts are configuration data, not stored assets.

### Command System
Not applicable - no new commands introduced. Existing `lui/ui_spec` command benefits from improved shortcut handling.

### Asset System
Not applicable - shortcuts are metadata, not assets.

### Value Types
Not applicable - shortcuts are not ExtValue types. They are UI metadata similar to colors or fonts.

### Web/API (if applicable)
Not directly applicable for Phase 1. Future: could expose shortcut configuration via API for web-based UI customization.

### UI (if applicable)
**Primary integration point.** Used by:
- `UISpecElement` menu bars (replace current ad-hoc parsing in `ui_spec_element.rs:378-416`)
- Future widgets that need keyboard shortcut support (query console, text editor, custom panels)
- Conversions to `egui::KeyboardShortcut` (immediate), `crossterm::KeyEvent` (future), `dioxus::events::KeyboardData` (future)

## Crate Placement

**liquers-lib** - `src/ui/shortcuts.rs` module
- Rationale: UI utility, similar to existing UI modules (element, app_state, widgets)
- Dependencies: egui (already in liquers-lib), optional ratatui/dioxus feature flags for future
- No changes to liquers-core, liquers-store, or liquers-axum

## Design Decisions

1. **No chord sequences** - Single shortcuts only (e.g., "Ctrl+S"), not multi-step sequences (e.g., "Ctrl+K Ctrl+S")
2. **Data structures only** - No global shortcut registry (platform-dependent), callers manage their own registries
3. **Library-level conflict detection** - Provide utilities to detect duplicate shortcuts
4. **Web format compatibility** - Support both "Ctrl+S" format and browser "Control+KeyS" format for parsing and serialization

## Open Questions

None - all design decisions resolved.

## References

- Research: `/home/orest/zlos/rust/liquers/.claude/skills/liquers-designer/KEYBOARD_SHORTCUTS_*.md`
- Current implementation: `liquers-lib/src/ui/widgets/ui_spec_element.rs:378-416` (check_shortcut, parse_key)
- egui docs: https://docs.rs/egui/latest/egui/struct.KeyboardShortcut.html
- crossterm docs: https://docs.rs/crossterm/latest/crossterm/event/struct.KeyEvent.html
- Dioxus keyboard docs: https://docs.rs/dioxus/latest/dioxus/events/struct.KeyboardData.html
