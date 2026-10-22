---
id: TEXT-EDITOR-EDITS-ARE-NEVER-SAVED
kind: feature
title: The text_editor widget edits a copy of the text and never writes it back
status: draft
priority: P3
complexity: M
area: [lib/egui, lib/ui]
design: text-editor-save
created: 2026-10-19
github:
---
## Problem

The `text_editor` command (`liquers-lib/src/egui/commands.rs`) shows a `TextEditor`
(`liquers-lib/src/egui/widgets.rs`) for the text stored under a key. The widget loads the text into
its own `String` and binds it to an `egui::TextEdit`, but nothing ever writes the edited text back
to the store or the key's asset: there is no save action, message or command. Closing the view or
re-evaluating the query discards every edit.

## Impact

The command's label ("Text editor") and doc ("for editing text identified by a key") promise an
editor; users get a scratch buffer. No data is lost that was saved before, so there is a workaround
(copy the text out), which keeps this at P3.

## Expected behaviour

An explicit save (button or shortcut) writes the text to the key — for example by sending an
`AppMessage` the runner turns into a store write or an asset `set`, or through a store-key accessor
(`specs/design/value-accessor/`). Alternatively, the command is renamed or documented as a viewer.

## Discovery

Found while designing `specs/design/ui-asset-snapshot-boundary/` (EGUI-ASSET-MANAGER-INTEGRATION),
which moves the widget to a `TextEditorElement` without changing this behaviour.

## Problem example

Added 2026-10-21 (bulk design; reproduced at `8ecfd7f`). With `notes/readme.txt` = `hello notes`,
a node showing `-R-key/notes/readme.txt/-/text_editor`, the user appends ` and more`; no save
exists, and re-evaluating shows `hello notes`. Designed in `specs/design/text-editor-save/`
(after `ui-asset-snapshot-boundary` creates `TextEditorElement`).
