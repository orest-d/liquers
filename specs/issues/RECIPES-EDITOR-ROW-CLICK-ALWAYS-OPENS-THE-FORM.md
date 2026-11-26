---
id: RECIPES-EDITOR-ROW-CLICK-ALWAYS-OPENS-THE-FORM
kind: feature
title: A click on a row of the recipes editor always opens Liquers' recipe form, and a host cannot send it to its own editor
status: closed
priority: P2
complexity: S
area: [lib/ui, lib/egui]
design: recipes-editor-open-action
created: 2026-11-26
github:
---
## Problem

`RecipesElement` turns a click on a row (egui `selectable_label`, web `rc:open:<file>`) and the
row menu's Edit into the same `RecipesCommand::Open(file)`, which `RecipesState::apply` answers by
opening its recipe window (`liquers-lib/src/ui/recipes/state.rs`, design `recipes-editor` AC-4).
That window's form writes the recipe and has no preview. A host with a better editor for a node
cannot be reached from the click: in egui the click is turned into the command inside the drawing
code, so no event reaches an element wrapping the panel.

Found by DPL (orest-d/dpl design `project-node-stopgap`, decision 6b): a click on a node of a
project should edit it in DPL's recipe workbench, which has Live preview and a 3D view.

## Expected behaviour

A host can name a command (namespace and name, given to `lui/recipes_editor`) that a row click runs on the clicked node; the row
menu's Edit keeps opening the form.

## Resolution

Design `recipes-editor-open-action`: `lui/recipes_editor-<namespace>-<command>` names a command a
row click runs on the node's key; the row menu's Edit opens the form.
