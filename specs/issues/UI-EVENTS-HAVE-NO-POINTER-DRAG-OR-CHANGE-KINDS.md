---
id: UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS
kind: feature
title: UI events cannot express field changes, focus, drag-and-drop or pointer input
status: draft
priority: P3
complexity: L
area: [lib/ui]
design: ui-events
created: 2026-10-30
github:
---

## Problem

The first `ui-events` increment (`specs/design/ui-events/`) defines `UIEventKind` with only
`Submit` and `Named(String)`. A click on a node marked `data-lq-event` is covered by `Named`, but
nothing can express:

- a field value committed without submitting (checkbox, select, text field on blur);
- focus entering or leaving an element;
- something dropped onto an element (a key, a query, a file) or a drag started from one;
- raw pointer input: position, button, double-click, hover, wheel.

The web driver (`liquers-lib/src/ui/web/app.rs`) listens only to `click` and `keydown`, and the
imperative per-element mount hook that canvas-like widgets would need is a non-goal of the first
increment.

## Impact

Forms with non-text controls, drag-and-drop between widgets (dropping a key onto a query console
or a view is a natural Liquers interaction) and any canvas-style widget cannot be built on the
shared vocabulary; they would fall back to `UpdateMessage::Custom`, a private per-element contract.
No workaround inside the vocabulary.

## Expected behaviour

New backend-neutral kinds, following the design's platform-independence principle (no `web_sys`,
egui or terminal types; pointer coordinates element-local and logical). A suggested order:

1. `Change` — field committed, carrying the field values as today.
2. `Drop` — dropped payload carried in the fields as a `Value` (key, query text, file).
3. `Focus` / `Blur`.
4. Raw pointer and canvas input, which needs the per-element mount hook and its own design.

Key routing between nested hosts (ratatui inside egui inside web) is a related driver concern.

## Discovery

Review of the `ui-events` design with Orest on 2026-10-30, while confirming what the first
increment covers.
