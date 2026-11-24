---
id: QUERY-EDITOR-HAS-NO-LIVE-PREVIEW
kind: feature
title: A query editor can write each edit upstream or hold it, but cannot preview an edit's result without writing it
status: draft
priority: P2
complexity: L
area: [lib/ui, lib/egui]
design: query-live-preview
created: 2026-11-24
github:
---
## Problem

The editors of `query-pipeline-editor` (`EditorElement`, `liquers-lib/src/ui/editor/`) have two
update models (AC-26): "Update immediately" writes each committed edit into the accessor, and
otherwise edits wait, pending, for Update. There is no third model in which an edit is evaluated
and shown, but not written, until the user confirms it, and there is no undo.

Writing every edit of a recipe's query rewrites `recipes.yaml`, expires everything that depends on
the recipe and recomputes every open view of it, for intermediate states the user never meant to
keep. Holding the edits instead leaves the user editing blind.

The query console shows a value but re-evaluates it only on Submit, and swaps the value for an
empty view the moment it submits (`submit_query` clears `value`), so a fast result flickers and a
slow one leaves the user with nothing but a status word.

## Expected behaviour

An editor can preview the edited query's result without writing it, apply or revert it, and undo
an edit; the console shows a new value without flicker when it is quick, and a status bar with
progress, plus the old value dimmed, when it is not. Design: `specs/design/query-live-preview/`.
