---
id: QUERY-CONSOLE-EDITOR-LOSES-EDITS-WHEN-UPDATE-IMMEDIATELY-IS-OFF
kind: issue
title: The query console's editor panel drops an edit made with "Update immediately" off, and never re-evaluates
status: draft
priority: P2
complexity: M
area: [lib/ui]
design: query-apply-immediately
created: 2026-11-24
github:
---
## Problem

The query console's editor panel (`QueryConsoleElement`, `liquers-lib/src/ui/widgets/query_console_element.rs`)
draws the editor's whole toolbar, so it shows the "Update immediately" checkbox of
`query-pipeline-editor` AC-26. In the console that checkbox has no meaning, and it misbehaves:

1. **On** (the default): an accepted edit rewrites `query_text`, but nothing is submitted
   (`apply_parameter_commands`; "an edit rewrites the text and does not submit",
   `UI_INTERFACE_FSD.md`). The value shown stays the value of the old query, although the user
   asked for an immediate update.
2. **Off:** an edit makes the editor's working copy pending. On the next frame
   `parameters_tree` sees that the working copy (`EditorState::value`) differs from `query_text`
   and rebuilds the editor from the text. The edit is lost and the checkbox is back on.

## Problem Example

Console query `-R/a.csv/-/head-3`, panel open. Untick "Update immediately", set `n` to `7`:
the field shows `3` again, `query_text` is still `-R/a.csv/-/head-3`, "Update" has nothing to
write, and the checkbox is ticked. Checked on 2026-11-24 (on top of PR #284) with a probe test
that applied `SetContinuous(false)`, `SetField n=7` and `Update` through
`apply_parameter_commands` and read `parameters_tree` after each.

## Expected behaviour

Every accepted panel edit rewrites the console's query text, whatever the setting. With "Apply
immediately" on, the console then re-evaluates (debounced); with it off, it re-evaluates on the
✓ button beside the query field or Enter. The setting is the console's own, shown once in the
console's toolbar; the panel shows no second switch or Update button.
Design: `specs/design/query-apply-immediately/`.
