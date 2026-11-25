---
id: PREVIEW-IS-FRAMED-ONLY-IN-THE-QUERY-CONSOLE
kind: feature
title: A Live preview is framed and ended only when the followed view is a query console
status: draft
priority: P3
complexity: M
area: [lib/ui, lib/egui]
created: 2026-11-24
github:
---
## Problem

A query editor in Live preview (`specs/design/query-update-modes/` Part B) sends its preview to the
view it follows (`AppMessage::Preview`). Only `QueryConsoleElement` draws such a value through a
`ValueDisplay`: framed in the theme's preview colour, with its status line, and back to its own
query on `lq:preview-end`. Any other followed view, such as `AssetViewElement` or a Markdown view,
shows the preview's value as if it were the applied one, and ignores `lq:preview-end`, so after a
Revert it keeps showing the dropped preview until something else re-points it.

## Problem Example

A UI spec puts an asset view of `-R/reports/loud.txt` beside a `lui/query_editor` of its recipe,
following the view. An edit in Live preview replaces the view's value with no frame; Revert leaves
the preview on screen.

## Expected behaviour

Every element that can be followed draws a preview through `ValueDisplay` (frame, status line) and
handles `lq:preview-end` by asking for its own source query again.
