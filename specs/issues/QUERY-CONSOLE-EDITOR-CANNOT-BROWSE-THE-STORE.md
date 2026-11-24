---
id: QUERY-CONSOLE-EDITOR-CANNOT-BROWSE-THE-STORE
kind: issue
title: The query console's editor panel opens a store browser that never lists anything
status: closed
priority: P3
complexity: S
area: [lib/ui]
design: query-update-modes
created: 2026-11-24
github:
---
## Problem

A resource card in the query console's editor panel has a browse button (`EditorCommand::Browse`).
The editor answers it with `EditorEffect::ListDirectory(folder)`, but
`QueryConsoleElement::apply_parameter_commands`
(`liquers-lib/src/ui/widgets/query_console_element.rs`) drops that effect, so no listing is
requested and the "Choose a key" window stays at "Listing…". The standalone `EditorElement`
handles the effect (it asks the runner for the directory and feeds `on_directory` back); the
console does not.

## Impact

In the console the key of a resource must be typed. "Add key…" on an empty query (added on
2026-11-24) inserts a resource with an empty key, which a working browser would make easy to fill.

## Expected behaviour

The browse button in the console lists the store like the standalone editor does: the console
sends the same directory request for its handle and passes `UpdateMessage::Directory` to its
editor state.

## Resolution

The console now sends `AppMessage::ListDirectory` for the panel's `EditorEffect::ListDirectory` and passes the `UpdateMessage::Directory` answer to the panel. Evidence: `browse_lists_the_store` in `liquers-lib/src/ui/widgets/query_console_element.rs` (design `query-update-modes` Part A, PR #285).
