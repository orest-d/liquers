# Phase 1: High-Level Design — Query and pipeline editor

## Purpose

Editing a query today means editing one line of text and knowing each command's arguments and
the parameter grammar by heart, although the command metadata registry declares a label, a
description, a type, a default and a widget (`gui_info`) for every argument. This design adds
`UIElement` editors that use the registry and edit through value accessors
(`settable-value-accessor`): a **query / pipeline editor** (a collapsible tree of steps beside the
highlighted text), an **action editor** (one action's arguments with their declared widgets and
documentation on hover), a **resource editor** (a resource's header and key, with a store file
selector) and a **key editor** (the key alone), for the egui app and the
browser (VISION: queries are the user interface).

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
▾ resource   [      ▾][data/sales.csv     ][📁]
▸ ns         pl
▸ from_csv                                         [presets ▾][next ▾][filename]
▾ is_in      Filter rows where column equals one of the values      [presets ▾][next ▾]
     column  [region      ] [↺]
     values  [EU] [US] [+]   [↺]
▾ head       Return first N rows (default: 5)                         [presets ▾][next ▾]
     n       ▾ link                                                    [value|link] [↺]
               ▸ resource  config/limit.txt
               ▸ to_text
[+ action]
filename     [                  ] [Default]
```

`[+ action]` opens a searchable tree of namespaces and their commands. Hovering `head` shows its `doc`; hovering `n` shows its description, type and default. Adding
`APAC` to `values` writes `…/is_in-region-EU-US-APAC/…` back to `recipes.yaml`, and the text field,
the tree and any other editor bound to the same recipe show it.

## Scope and Acceptance Criteria

- **AC-1** The query as a tree
  WHEN a query editor is opened on the problem example
  THEN it shows a resource node `data/sales.csv`, an `ns` node `pl` and action nodes `from_csv`,
  `is_in` and `head`, each collapsible, and `head`'s argument `n` is a link node that expands into
  its own resource and action nodes
- **AC-2** Widgets from the registry
  WHEN an action node is expanded
  THEN it shows one field per argument that is not injected and whose `gui_info` is not `Hide`, in
  declaration order, with the widget `gui_info` names; when `gui_info` is the default text field the
  argument type picks the widget (checkbox, integer, float, enum selector); an enum writes the chosen
  alternative's alias, a `GlobalEnum` lists its alternatives, and a variadic argument is an ordered
  list of its values (one widget per item) with append, delete and reorder by drag and drop
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
- **AC-6** Insert, delete and append
  WHEN the user deletes a node, inserts an action or a resource before or after a node, or appends an
  action at the end
  THEN the query changes accordingly, through the same validated edits as AC-5. Moving a node up or
  down is optional (Phase 2 decides whether the first version has it)
- **AC-7** Resource editor
  WHEN a resource node (anywhere in a query or pipeline) is edited: its header, chosen from a
  dropdown placed before the key field (as the header precedes the key in the query), which lists
  every header the planner accepts without the `-R` (empty for the asset, then `key`, `dir`,
  `recipe`, `cwd`, `binary`, `metadata`, `sdir`, `stored_binary`, `stored_metadata`), and its key
  THEN the resource changes and the rest of the query is kept. The header changes the element kind
  where it must: `key` passes the key itself (`UseKeyValue`) and `cwd` sets the working directory,
  neither reads anything, and the editor says so
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
- **AC-11** Both monolithic backends
  WHEN the same editor element is used in the egui app and in a monolithic browser UI
  THEN it behaves as the other criteria say in each. (Split mode is not built here: the editor
  reaches its value only through the accessor, so a later split design can run it in the client
  with a copy of the registry and an accessor channel, unchanged.)
- **AC-12** Old state loads
  WHEN an app state saved before this change is loaded
  THEN it loads unchanged
- **AC-13** Controllers drive numeric arguments
  WHEN an outside event source (a MIDI knob, a shortcut, a script) sets "numeric argument 2 of the
  edited action" to a value in its own range
  THEN the value is scaled to the argument (an integer as is, a float into its slider range or 0-1),
  written through the same validated edit and binding, and the editor and any view of the query's
  result update
- **AC-14** The key as a file selector
  WHEN the user opens the key field of a resource editor or a key editor
  THEN it browses the store's directory listing (folders to open, keys to pick), and also accepts any
  key typed directly, including one that does not exist yet
- **AC-15** Key editor
  WHEN a key editor element is opened on a key, or an argument's link is a `-R-key/…` query
  THEN it is the resource editor's key field and file selector without the header choice (a key
  argument needs no gui hint of its own: the `-R-key` link is what makes it a key)
- **AC-16** Directives
  WHEN the query contains `ns`, `q` or `v`
  THEN each is a node of its own; `q` and `v` can be inserted and removed; `ns` is an ordered list
  editor (reorder by drag and drop, delete, and append one of the valid namespaces not used yet);
  the text shows the same directives
- **AC-17** Searchable command tree
  WHEN the user inserts or appends an action
  THEN a searchable tree of namespaces and their commands (label, name, `doc` on hover) is offered,
  and picking a command outside the active namespaces also inserts the `ns` directive it needs
- **AC-18** Reset to default
  WHEN the user resets an argument
  THEN its parameter is removed when it is the last one written, otherwise the argument's default is
  written in its place, and the field shows the default
- **AC-19** A link as an argument
  WHEN the user switches an argument from value to link
  THEN it becomes a link node holding a query (a new resource or action subtree) and is written as
  `~X~…~E`; switching back writes the argument's default
- **AC-20** Action presets
  WHEN the user picks an entry of the presets menu beside an action
  THEN the action is replaced by that preset (`CommandMetadata::presets`)
- **AC-21** Next presets
  WHEN the user picks an entry of the next menu beside an action
  THEN the preset action (`CommandMetadata::next`) is inserted right after that action, with its
  `ns` when it needs one
- **AC-22** Filename
  WHEN the user presses the filename button beside an action that declares `filename`
  THEN that filename is set as the query's filename; WHEN the user edits the filename field after the
  last action, THEN the query's filename is that text; WHEN the last command declares `filename`,
  THEN a Default button beside the field sets it, and WHEN it declares none, THEN there is no
  Default button (the editor never evaluates the query to learn its result type)
- **AC-23** Excess parameters and unknown commands
  WHEN an action has more parameters than its arguments take, or names a command the registry does
  not know
  THEN the extra parameters are shown read-only, or the node shows the action's text and the reason,
  and edits elsewhere in the query still work
- **AC-24** Argument presets
  WHEN an argument declares presets (`ArgumentInfo::presets`) and the user picks one from the small
  presets menu beside its field
  THEN the argument is set to that preset's value (a literal or a link), through the same validated
  edit as AC-5
- **AC-25** Safe namespace edits
  WHEN an `ns` edit keeps every affected action resolving to the same command
  THEN it is applied; WHEN every action still resolves and its arguments still fit, but some
  resolve to a different command (`pl` swapped for `rec`), THEN the editor names those actions and
  applies the edit only after the user confirms; WHEN an action no longer resolves or its arguments
  no longer fit, THEN the edit is refused with the reason (AC-5)
- **AC-26** Continuous or on-demand update
  WHEN "Update immediately" is on
  THEN each committed edit is set into the accessor at once, so its change event follows and views
  of the result recalculate; WHEN it is off, THEN edits stay in the editor, marked pending, until the
  user presses Update (or Revert, which reloads the accessor's value)

Non-goals: per-keystroke planning diagnostics in the text field (`query-editor-live-validation`);
evaluating the query inside the editor (a view bound to the same query or recipe re-evaluates because the binding expires it, as in AC-13); the MIDI input itself (AC-13 defines only the numeric-argument addressing an event source uses); placeholders and command-definition
mode (`WEB-PIPELINE-EDITOR-HAS-NO-DEFINITION-MODE`); drag and drop in the browser (egui has it; the browser gets buttons until `UI-EVENTS-HAVE-NO-POINTER-DRAG-OR-CHANGE-KINDS` adds drag events); a date picker (`DateField` is a
text field); Python and JavaScript constructors for the elements beyond what the `lui` commands give.

## Core Interactions

- **Commands:** `CommandMetadataRegistry` (`CommandMetadata::doc`, `ArgumentInfo` label,
  description, type, default, `gui_info`; `CommandMetadata::presets`, `next` and `filename`; the
  namespaces for the command tree), read only.
- **Store:** directory listings for the key selector (`listdir` through the asset manager, so
  recipe-only keys are offered too; Phase 2 confirms the call).
- **Query / pipeline:** `QueryEditor` / `PipelineEditor` (`liquers-core/src/pipeline/editor.rs`) for
  every edit and its refusal; `Pipeline`, `ElementKind` (resource, key, cwd, action, `ns`, `q`, `v`)
  and `Source` for the tree, `Source::Link` for subtrees; the query's filename.
- **Value accessors:** the editor's binding (`liquers-core/src/accessor/`): a query, recipe or action
  accessor, `ActionStep` / `ArgumentStep` projections, `set_if` conflicts, `subscribe` for changes. A
  query that lives nowhere else is a memory accessor.
- **UI:** four new `UIElement`s (query / pipeline, action, resource, key editors), their egui rendering (reusing `edit_query`), their `render_web`
  markup and `ui-events` user events, and `lui` commands that create them.

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
  `scripts/docs_index.py chapters-for`); `NO-COMMAND-PARAMETER-EDITOR` (closed in Phase 5);
  `UI-VARIADIC-ARGUMENT-LIST-EDITOR` (its list widget is delivered here; the issue stays with
  `variadic-argument-slots`).
- API docs: the new element and tree-model modules under `liquers-lib/src/ui/widgets/`,
  `liquers-lib/src/ui/widgets/mod.rs`, and `liquers-lib/src/egui/widgets.rs` (new editor widgets).

## Design Dependencies

- **Requires:** `settable-value-accessor` (complete) for the binding; `ui-events` (complete) for the
  browser's user events.
- **Later:** `ui-split-elements` (Phase 1 in review, another thread; Orest expects a larger
  overhaul). Not required: this design is monolithic, and the editor touches its value only through
  the accessor.
- **Requires for AC-26 with a recipe binding:** `recipe-dependency-versions` (in review, not
  implemented; issue `RECIPE-DEPENDENCY-KEY-IS-NEVER-VERSIONED`, P2). Today a write of
  `recipes.yaml` does not expire the assets computed from its recipes, so a view of the recipe's
  result does not recalculate after an edit. The editor and other holders of the accessor do see
  the change.
- **Supersedes:** `command-parameter-editor` (Orest, 2026-11-16; not implemented). It owned
  `NO-COMMAND-PARAMETER-EDITOR`, which moves here. What is kept is under "Salvaged from
  command-parameter-editor" below.
- **Overlaps:** `query-editor-live-validation` (in review, not implemented): the registry in
  `UIContext` (its Step 3) is needed here too; whichever lands first adds it. `variadic-argument-slots`
  (in review): the list edits of AC-2 use `ActionEditor::push` / `unset` today, and Phase 2 checks
  whether its slot edits are still needed. `template-editor-ui`: shares the highlighted text field.

## Open Questions

Answered by Orest on 2026-11-16: this design **replaces `command-parameter-editor`** (old question 1),
and the resource editor is a part of the query and pipeline editor as well as an element of its own,
with a header choice, a file selector and a key editor without the header (old question 2; now
AC-7, AC-14, AC-15). His notes also added directives (AC-16), the command tree (AC-17), reset
(AC-18), link arguments (AC-19), presets (AC-20), next (AC-21), the filename (AC-22) and made
moving optional (AC-6).

1. **Proposed resolution - always bind through an accessor.** A query typed in place is a memory
   accessor, so one code path handles editing, conflicts and change notification (AC-9).
2. **Proposed resolution - link arguments in place.** A link argument is a subtree of the same tree
   (AC-1, AC-19), edited with the same nodes, rather than a nested editor in a pop-up.
3. **Decided - split mode (Orest, 2026-11-17).** Monolithic only. The editor is a view and control
   connected to its model by the accessor; it is not itself split into halves (AC-11).
4. **Proposed resolution - controllers (AC-13).** In scope as the numeric-argument addressing and
   scaling only, taken from the proof of concept. Alternative: drop AC-13 for a later design.
Answered by Orest later on 2026-11-16: no key gui hint is needed, because a `-R-key/…` link acts as
a key (old question 5, now in AC-15); the Default filename button is hidden when the last command
declares no filename, because the query is never executed to learn its type (old question 6, now
in AC-22); argument presets get a menu (old question 8, now AC-24); the resource header is a
dropdown before the key field, showing the header word without `-R` and nothing for the asset
(old question 7, now AC-7).

5. **Implementation detail - argument presets in practice.** No command in the workspace or in the
   proof of concept declares argument presets yet (the proof of concept's colour is a `ColorString`
   widget, not a preset), so a test registers one.
6. **Implementation detail - moving nodes (AC-6).** Recommended: include up and down, since
   `PipelineEditor::move_to` exists.

## Scope Changes

- **2026-11-17, Orest's Phase 2 review.** Added AC-25 (an `ns` edit is applied when it keeps the
  meaning, confirmed when it changes which command an action resolves to, refused when it breaks the
  query) and AC-26 (a switch between updating the accessor immediately and an Update button).
  AC-2 and AC-16 now edit variadic arguments and `ns` as ordered lists with drag and drop. Size stays
  L. Phases 1 and 2 updated; this is part of the Phase 2 gate.

## Salvaged from command-parameter-editor

- The type-derived widget rule (its AC-5, here AC-2): 410 of 423 exported `gui_info` entries are the
  default `TextField(40)`, so the argument type has to pick checkbox, number or enum widgets.
- Its widget mapping over every `ArgumentGUIInfo` variant, egui and browser (`<input list>` with a
  `<datalist>` for enums, `type=number` / `range` / `color` / `date`), enum aliases, `GlobalEnum`
  resolution and free text when an enum allows others.
- Read-only excess parameters (its AC-8) and "no form, with a reason" (its AC-7): AC-23.
- Reset to default (its AC-4): AC-18.
- The query console's toggled Parameters panel (its AC-10): the console embeds the action editor of
  its last action (Phase 2).
- Its finding that command resolution must follow the planner, aliases included, with the next-preset
  lookup of `liquers-lib/src/utils.rs` (`find_next_presets`) as the fallback for a broken query.
- Dropped: `set_argument_parameter`, which duplicates `ActionEditor::set`.

## Proof of Concept

`orest-d/egui-midi-test` `src/editor.rs` (`edit_query`, `ExtendedQuery`, `ActionEditorRecord`), read
on 2026-11-16. Each frame it parses a text field, builds a two-column `egui::Grid` (label, widget)
for the **last** action from its `CommandMetadata`, and picks the widget from `gui_info`: text
fields sized from the width hint, sliders for `IntegerRange` / `IntegerSlider` / `FloatSlider`,
`t`/`f` checkboxes and radio pairs, and a colour picker that writes `RRGGBBAA` for `ColorString`.
Any change re-encodes the query and re-evaluates it. MIDI control changes address "the n-th numeric
parameter" of that action and scale 0-127 to an integer or to 0-1 (`find_numeric_parameter`,
`set_numeric_parameter_value_in_query`).

Kept here: the label-widget grid and the `gui_info` widget mapping (AC-2), the text field above the
form kept in step (AC-4), re-evaluation on change (through the binding), and numeric-argument
addressing for controllers (AC-13). Changed: any action rather than the last one (AC-1, AC-8);
parameters map to arguments through `PipelineEditor` rather than by position, so defaults,
variadic and link arguments and named parameters work and excess parameters are not an error;
an invalid entry is refused (AC-5) rather than read as `0` by `unwrap_or`; enum widgets and `Hide`
are implemented instead of `todo!()`; a radio pair writes only when clicked; diagnostics go to the
editor, not `println!`.

## References

- Designs: `settable-value-accessor`, `composite-command-definition` (pipelines and editors),
  `command-parameter-editor`, `query-editor-live-validation`, `ui-events`, `ui-split-elements`.
- Issues: `NO-COMMAND-PARAMETER-EDITOR`, `UI-VARIADIC-ARGUMENT-LIST-EDITOR`,
  `CORE-QUERY-EDITOR-ACCEPTS-AN-UNKNOWN-COMMAND`,
  `EDITOR-UNSET-OF-A-MIDDLE-POSITIONAL-ARGUMENT-SAYS-NOT-SET`.
- References: `specs/reference/UI_INTERFACE_FSD.md`, `specs/reference/VALUE_ACCESSORS.md`,
  `specs/reference/PIPELINES.md`.
