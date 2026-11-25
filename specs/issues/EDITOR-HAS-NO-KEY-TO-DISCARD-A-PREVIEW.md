---
id: EDITOR-HAS-NO-KEY-TO-DISCARD-A-PREVIEW
kind: feature
title: Escape does not discard a Live preview in the query editor
status: draft
priority: P3
complexity: S
area: [lib/egui]
created: 2026-11-24
github:
---
## Problem

`specs/design/query-update-modes/` Phase 2 planned `EditorKey::Discard` (Esc) to drop a Live
preview. Part B did not add it: Revert and Undo drop a preview, and the status line says so, but
there is no single key for it, and Esc on a card header already collapses the card.

## Expected behaviour

A key (Esc where nothing else takes it, or another) reverts the pending edits of an editor in Live
preview, listed in `editor_keymap()` and the user guide's key table.
