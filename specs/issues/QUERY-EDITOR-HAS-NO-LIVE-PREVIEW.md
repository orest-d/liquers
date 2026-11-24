---
id: QUERY-EDITOR-HAS-NO-LIVE-PREVIEW
kind: feature
title: A query editor cannot show an edit's result without writing it upstream
status: draft
priority: P2
complexity: L
area: [lib/ui, lib/egui]
design: query-update-modes
created: 2026-11-24
github:
---
## Problem

The editors of `query-pipeline-editor` (`EditorElement`, `liquers-lib/src/ui/editor/`) either write
each committed edit into their accessor ("Update immediately", AC-26) or hold it until Update. There is no **live preview**: a mode in which
the edited query is evaluated and its value shown while the accessor stays unchanged until the user
confirms.

For an editor bound to a recipe of a `recipes.yaml`, "Apply immediately" rewrites the file on each
edit, which expires the recipe's dependents and recomputes every open view of it, for intermediate
states nobody meant to keep. Turning it off avoids that, but then the user sees no result until
Update.

## Problem Example

`lui/query_editor` bound to `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query`, beside a
view of the recipe's result. Dragging a slider of the recipe's last action with "Apply immediately"
on writes `recipes.yaml` once per accepted value; with it off, the view stays on the old result.

## Expected behaviour

A third choice for editors whose accessor is expensive to write: the edited query is evaluated in
the recipe's folder (its cwd) and shown in the followed view or a preview pane, marked as a
preview, while the accessor is written only on ✓ / Enter, so the impact on one recipe is seen
before its change cascades into costly dependents. Orest, 2026-11-24: editors offer all three
modes (on demand, live preview, apply immediately). Design: `specs/design/query-update-modes/`.
