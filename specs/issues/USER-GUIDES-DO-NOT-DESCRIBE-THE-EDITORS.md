---
id: USER-GUIDES-DO-NOT-DESCRIBE-THE-EDITORS
kind: issue
title: The user guides' UI chapters do not describe the query, action, resource and key editors
status: draft
priority: P3
complexity: S
area: [docs]
design:
created: 2026-11-17
github:
---
## Problem

`docs/guides/{rust,python,web}/11-ui.md` describe the UI elements but not the editor elements of
`specs/design/query-pipeline-editor/` (`lui/query_editor`, `action_editor`, `resource_editor`,
`key_editor`, the query console's Parameters panel). Their `sources:` would be
`specs/reference/UI_INTERFACE_FSD.md` §Editors and `specs/guides/VALUE_ACCESSOR_GUIDE.md`
§Binding an editor.

## Impact

A user of the guides learns of the editors only from the specs. Workaround: read those sections.

## Expected behaviour

A section in each 11-ui chapter: binding an editor to a recipe query, continuous versus Update, and
what the command metadata contributes (labels, `doc`, `gui:`, presets, `next`, `filename`).

## Discovery

Phase 5 of `query-pipeline-editor`, 2026-11-17: `docs_index.py chapters-for` lists the chapters; they
remain true but say nothing of the editors.
