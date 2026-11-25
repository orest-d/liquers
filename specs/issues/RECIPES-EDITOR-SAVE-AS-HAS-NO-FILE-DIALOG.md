---
id: RECIPES-EDITOR-SAVE-AS-HAS-NO-FILE-DIALOG
kind: issue
title: Save as in the egui recipes panel asks for a typed path instead of a file dialog
status: draft
priority: P3
complexity: S
area: [lib/egui]
design:
created: 2026-11-24
github:
---
## Problem

The design of the recipes panel (`design/recipes-editor/`, Q8) planned a native file dialog (`rfd`)
for Save as and Export. The implementation asks for the path in a small egui window instead,
prefilled with the file's name: `rfd` pulls in GTK or the XDG portal (zbus) on Linux, a large
dependency and a system library the cloud and CI builds do not have.

## What would fix it

`rfd` as an optional dependency behind its own feature (say `egui-file-dialog`), used by
`egui::recipes` when enabled, with the typed path kept as the fallback.
