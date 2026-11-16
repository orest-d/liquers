# Phase 1: High-Level Design — Query and pipeline editor

## Purpose

Editing a query today means editing one line of text and knowing each command's arguments and
the parameter grammar by heart, although the command metadata registry declares a label, a
description, a type, a default and a widget (`gui_info`) for every argument. This design adds
`UIElement` editors that use the registry and edit through value accessors
(`settable-value-accessor`): a **query / pipeline editor** (a collapsible tree of steps beside the
highlighted text), an **action editor** (one action's arguments with their declared widgets and
documentation on hover) and a **resource editor** (the `-R/…` segment), for the egui app, the
browser and the split UI of `ui-split-elements` (VISION: queries are the user interface).

## Problem Example

A recipe in `reports/recipes.yaml` holds this query (validated with `liquers-validate`, `Ok`):

```
-R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-~X~-R/config/limit.txt/-/to_text~E
```

**Today.** The only editor is the query console's text field (`QueryConsoleElement`, drawn with
`edit_query` in `liquers-lib/src/egui/widgets.rs`, which highlights the syntax). To change `head`'s
row count the user must know that `n` is written as a link `~X~…~E`; to add `APAC` to `is_in` they
must know `values` is variadic and comes after `column`. Nothing shows `head`'s documentation, and
no UI reads `gui_info` (`NO-COMMAND-PARAMETER-EDITOR`). The console edits a string it owns, so it
cannot write the query back into `recipes.yaml`, and a second editor of the same recipe would not
see the change.

**Wanted.** `-R-key/reports/recipes.yaml/-/ns-acc/accessor/recipe-0/query/ns-lui/query_editor`
(command name to be confirmed in Phase 2) opens an editor bound to that recipe's query:

```
[ -R/data/sales.csv/-/ns-pl/from_csv/is_in-region-EU-US/head-~X~…~E ]   ← highlighted text
▾ resource   data/sales.csv
▸ from_csv                                   (pl)
▾ is_in      Filter rows where column equals one of the values
     column  [region      ]
     values  [EU] [US] [+]
▾ head       Return first N rows (default: 5)
     n       ▾ link
               ▸ resource  config/limit.txt
               ▸ to_text
```

Hovering `head` shows its `doc`; hovering `n` shows its description, type and default. Adding
`APAC` to `values` writes `…/is_in-region-EU-US-APAC/…` back to `recipes.yaml`, and the text field,
the tree and any other editor bound to the same recipe show it.

## Scope and Acceptance Criteria

- **AC-1** The query as a tree
  WHEN a query editor is opened on the problem example
  THEN it shows a resource node `data/sales.csv` and action nodes `from_csv`, `is_in` and `head`, each
  collapsible, and `head`'s argument `n` is a link node that expands into its own resource and
  action nodes
- **AC-2** Widgets from the registry
  WHEN an action node is expanded
  THEN it shows one field per argument that is not injected and whose `gui_info` is not `Hide`, in
  declaration order, with the widget `gui_info` names; when `gui_info` is the default text field the
  argument type picks the widget (checkbox, integer, float, enum selector), and a variadic argument
  is an ordered list with add, remove and move
- **AC-3** Documentation on hover
  WHEN the user hovers an action header or an argument label
  THEN the command's label and `doc`, or the argument's label, description, type and default, are
  shown
- **AC-4** Text and tree stay in step
  WHEN the user edits the tree
  THEN the highlighted text shows the new query; WHEN the user edits the text and it parses, THEN
  the tree is rebuilt from it; WHEN it does not parse, THEN the error position is highlighted, the
  tree keeps the last valid query and is marked out of date, and nothing is written
- **AC-5** Refused edits change nothing
  WHEN an edit is refused by the registry (`n` set to `abc`, an unknown argument, a step that would
  make the query unrepresentable)
  THEN the editor shows the reason at that field, and neither the query nor its source changes
- **AC-6** Structural edits
  WHEN the user adds an action after a node (picked from the registry, the command's `next` presets
  first), removes one, or moves one up or down
  THEN the query changes accordingly, through the same validated edits as AC-5
- **AC-7** Resource editor
  WHEN the resource node is edited (its key, and whether it reads the data or the asset, `-R` or
  `-R-key`)
  THEN the query's resource segment changes and the rest of the query is kept
- **AC-8** Standalone action editor
  WHEN an action editor is opened on an action accessor (`…/query/action-2`)
  THEN it shows that action's form only (AC-2, AC-3), and its edits are written to where the query
  lives
- **AC-9** Bound to where the query lives
  WHEN two editors are bound to the same recipe's query and one of them changes it
  THEN the other shows the change; WHEN both change it from the same starting point, THEN the second
  edit is reported as a conflict and that editor reloads, and nothing is lost silently
- **AC-10** Pipelines
  WHEN the editor is bound to a recipe pipeline that is not representable as a query
  THEN the tree edits it as in AC-1 to AC-6, and the text field is read-only with the reason
- **AC-11** Two deployments
  WHEN the same editor element is used in the egui app, in a monolithic browser UI, and in a split
  (server engine, browser client) UI
  THEN it behaves as AC-1 to AC-10 in each; in split mode the client renders without the command
  registry, and every edit travels to the server as an event that the server validates
- **AC-12** Old state loads
  WHEN an app state saved before this change is loaded
  THEN it loads unchanged

Non-goals: per-keystroke planning diagnostics in the text field (`query-editor-live-validation`);
evaluating or previewing the query (the console does that); placeholders and command-definition
mode (`WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`); drag and drop; a date picker (`DateField` is a
text field); Python and JavaScript constructors for the elements beyond what the `lui` commands give.

## Core Interactions

- **Commands:** `CommandMetadataRegistry` (`CommandMetadata::doc`, `ArgumentInfo` label,
  description, type, default, `gui_info`, `next` presets), read only.
- **Query / pipeline:** `QueryEditor` / `PipelineEditor` (`liquers-core/src/pipeline/editor.rs`) for
  every edit and its refusal; `Pipeline` and `Source` for the tree, `Source::Link` for subtrees.
- **Value accessors:** the editor's binding (`liquers-core/src/accessor/`): a query, recipe or action
  accessor, `ActionStep` / `ArgumentStep` projections, `set_if` conflicts, `subscribe` for changes. A
  query that lives nowhere else is a memory accessor.
- **UI:** three new `UIElement`s, their egui rendering (reusing `edit_query`), their `render_web`
  markup and `ui-events` user events, and `lui` commands that create them.
- **Split UI:** the server and client halves of `ui-split-elements`, once that design defines them.

## Crate Placement

`liquers-lib`: the elements and a backend-free tree model in `src/ui/widgets/`, the egui drawing in
`src/egui/`, the markup in `src/ui/web/`, and the `lui` commands in `src/ui/commands.rs`, because
`UIElement` and both backends live there. `liquers-core` changes only if an edit the tree needs is
missing from `PipelineEditor` (Phase 2 lists them; `liquers-core` stays UI-free).

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` with "Query, action and resource editors"
  (the elements, their binding and their events).
- Guide: extend `specs/guides/VALUE_ACCESSOR_GUIDE.md` with binding an editor to an accessor; one
  line in `specs/guides/COMMAND_REGISTRATION_GUIDE.md` that `label`, `doc`, `description` and
  `gui_info` are now what users see in the editors.
- Other documents: none.
- Documents to update: the user guides' UI chapter if one lists the `lui` elements (Phase 5 runs
  `scripts/docs_index.py chapters-for`); `NO-COMMAND-PARAMETER-EDITOR` and
  `UI-VARIADIC-ARGUMENT-LIST-EDITOR` if question 1 is accepted.
- API docs: the new element and tree-model modules under `liquers-lib/src/ui/widgets/`,
  `liquers-lib/src/ui/widgets/mod.rs`, and `liquers-lib/src/egui/widgets.rs` (new editor widgets).

## Design Dependencies

- **Requires:** `settable-value-accessor` (complete) for the binding; `ui-events` (complete) for the
  browser's user events.
- **Required for AC-11 split mode:** `ui-split-elements` (Phase 1 in review, another thread). This
  design follows its Phase 1 decisions (the element knows its mode from `UIContext`; a server half
  and a client half; client messages are events). Monolithic mode does not wait for it.
- **Strong overlap, proposed to be superseded:** `command-parameter-editor` (in review, not
  implemented). Its last-action form is AC-2 to AC-5 here (T4), and its new `set_argument_parameter`
  duplicates `ActionEditor::set` (T2; the accessor design already said the form should edit
  through an argument accessor). Question 1.
- **Overlaps:** `query-editor-live-validation` (in review, not implemented): the registry in
  `UIContext` (its Step 3) is needed here too; whichever lands first adds it. `variadic-argument-slots`
  (in review): the list edits of AC-2 use `ActionEditor::push` / `unset` today, and Phase 2 checks
  whether its slot edits are still needed. `template-editor-ui`: shares the highlighted text field.

## Open Questions

1. **Open design - supersede `command-parameter-editor`?** Recommended: yes. This design's action editor is that
   form, generalised to any action and bound through accessors; its sources
   (`NO-COMMAND-PARAMETER-EDITOR`, `UI-VARIADIC-ARGUMENT-LIST-EDITOR`) move here, and the query
   console gets the action editor of its last action as a toggled panel (its AC-10) in Phase 2.
2. **Open design - "an editor of an action and a resource query".** I read this as two smaller editors used inside
   the query editor and on their own: the action editor (AC-8) and a resource editor (AC-7). If you
   meant one editor for a whole query that starts with a resource, AC-7 shrinks to the resource node.
3. **Proposed resolution - always bind through an accessor?** Recommended: yes. A query typed in place is a memory
   accessor, so one code path handles editing, conflicts and change notification (AC-9).
4. **Proposed resolution - link arguments in place.** Recommended: a link argument is a subtree of the same tree (AC-1),
   edited with the same nodes, rather than a nested editor in a pop-up.
5. **Proposed resolution - split mode timing.** Recommended: Phase 2 fixes the tree model as plain serializable data (what
   the client half renders) and implements monolithic mode first; the split halves are a Phase 4
   step that starts when `ui-split-elements` Phase 2 is approved.
6. **Open design (input for Phase 2) - the proof of concept.** I could not read `orest-d/egui-midi-test` `src/editor.rs` (the
   repository is not reachable from this session and the web page was rate limited). Phase 2 should
   take ideas from it: please add the repository to the project or copy the file into the project
   files.

## References

- Designs: `settable-value-accessor`, `composite-command-definition` (pipelines and editors),
  `command-parameter-editor`, `query-editor-live-validation`, `ui-events`, `ui-split-elements`.
- Issues: `NO-COMMAND-PARAMETER-EDITOR`, `UI-VARIADIC-ARGUMENT-LIST-EDITOR`,
  `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`,
  `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET`.
- References: `specs/reference/UI_INTERFACE_FSD.md`, `specs/reference/VALUE_ACCESSORS.md`,
  `specs/reference/PIPELINES.md`.
