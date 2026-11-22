---
id: WEB-EDITOR-HAS-NO-KEYBOARD-MODEL
kind: feature
title: The browser query editor has no keyboard shortcuts and cannot load icon images by key
status: draft
priority: P3
complexity: M
area: [lib/ui, web]
design: query-editor-ergonomics
created: 2026-11-21
github:
---
## Problem

The egui editor implements the keymap of `ui::editor::editor_keymap()` (Ctrl+B mode, Ctrl+J
jump, card navigation, Alt+1…9 presets, …). The browser renderer (`ui::web::editor`) draws the
same layout and sends the same `ed:*` events, but binds no key: the mount's keydown dispatcher
(`ui/web/app.rs` `dispatch_keydown`) handles menu shortcuts only. The browser also skips an icon
variant that names an image by store key (`IconSource::Key`; `icon_html` falls through to the
next variant or the Unicode text), because the mount has no loader for `AppMessage::LoadIcon`
results.

## Impact

Keyboard users of the browser console get the page's tab order only, and a theme or hint that
gives an icon only as a key shows its Unicode text in the browser. Workaround: give an inline or
set variant first.

## Expected behaviour

The mount dispatches editor keys from the same table (scopes from the focused element's
`data-lq-*` attributes), menu shortcuts first; Ctrl+J maps the text input's selection to a field
with `EditorView::jump_from_text`. Icon images by key are fetched through the runner's
`IconCache` and emitted as data URIs on the next render.

## Discovery

`query-editor-ergonomics` Phase 4, 2026-11-21: listed as a departure in Phase 2
§Implementation Changes.
