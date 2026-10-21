# Phase 1: High-Level Design — template-editor-ui

## Purpose

A template author should edit a stored template inside the Liquers UI, with syntax highlighting,
positioned errors and warnings while typing, saving to the store, and a preview of the rendered
result. Query fields anywhere in the UI should get the same live checking as the console. Serves
the template and interactive-UI use cases of `specs/reference/VISION.md`.

## Problem Example

Verified at HEAD (`8ecfd7f`): no element or command edits a template (`grep -ri template_editor`
over `liquers-lib`, `liquers-web` finds nothing); `text_editor` (`egui/commands.rs`) shows plain
text and never saves (`TEXT-EDITOR-EDITS-ARE-NEVER-SAVED`); query highlighting renders finished
queries only (`UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION`).

Concrete case: `templates/page.md` holds the template of `template-syntax-highlighter`'s example.
Today the author edits it in an external editor and learns of `unknown command 'field_extrct'`
only from `-R/templates/page.md/-/ns-template/validate` or a failed render. Should be: a node with
`-R-key/templates/page.md/-/ns-template/editor` (validated with the command declared) shows the
template highlighted, `field_extrct` underlined with its message as the author types; Ctrl+S saves
it; the preview pane below shows `-R/templates/page.md/-/ns-template/expand` (validated) and
refreshes after each save; with a preview input `-R/data/ctx.json` it shows
`-R/data/ctx.json/-/ns-template/render-~X~-R/templates/page.md~E` (validated) instead.

## Scope and Acceptance Criteria

- **AC-1** Editor command
  WHEN `-R-key/templates/page.md/-/ns-template/editor` is evaluated for a node
  THEN the node holds a text editor in template mode with the stored text
- **AC-2** Live diagnostics
  WHEN the editor's text contains `{{$ …/field_extrct-md}}` and the context carries the registry
  THEN the analysis has an error diagnostic at that action and the egui layout underlines it, with
  no evaluation
- **AC-3** Highlighting follows the buffer
  WHEN the text is edited
  THEN the laid-out galley's text is the buffer, highlighted with the template tokens
- **AC-4** Save
  WHEN the author saves
  THEN the key holds the new text (through `text-editor-save`)
- **AC-5** Evaluate-mode preview follows saves
  WHEN the editor is mounted with a handle and the template is saved
  THEN the preview pane receives a new snapshot of `<key>/-/ns-template/expand`
- **AC-6** Apply-mode preview
  WHEN the author sets the preview input to `-R/data/ctx.json`
  THEN the preview requests `-R/data/ctx.json/-/ns-template/render-~X~-R/templates/page.md~E`
- **AC-7** Preview errors visible
  WHEN the preview render fails
  THEN the preview pane shows the error, the editor stays usable
- **AC-8** Query fields elsewhere
  WHEN an egui form uses `edit_query_live` (from `query-editor-live-validation`)
  THEN it gets the same live checking as the console; the preview-input field is one such use

Non-goals: previewing unsaved text (open question 3); a browser editing surface (`render_web` shows
a highlighted read-only view); Markdown preview rendering beyond what `Value::show` does.

## Core Interactions and Crate Placement

UI (`liquers-lib/src/ui/widgets/text_editor_element.rs`): an editor mode; template analysis in
`liquers-lib/src/template/editor.rs` (feature `templates`); a command `template/editor`. Assets:
previews through `AppMessage::RequestAssetUpdates`. Template (`liquers-template`): tokens and
`validate_template`, unchanged here. No new crate or dependency.

## Documentation Intent

- Reference: `specs/reference/TEMPLATE_LANGUAGE.md` §4 lists `template/editor`;
  `specs/reference/UI_INTERFACE_FSD.md` "Editors" (modes, preview).
- Guide: `specs/guides/TEMPLATE_GUIDE.md` §1 "Editing in the UI".
- Other documents: `specs/command_registry.yaml` regenerated.
- Documents to update: the source issue.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — L; new command and public types (rules 1, 3, 4).
- **Leading issue:** **Open design - one editor element with modes** (`TextEditorElement` +
  `EditorSyntax { Plain, Template }`) versus a separate `TemplateEditorElement`. One element shares
  saving, layout and serialization; a separate one isolates the `templates` feature. Recommended:
  one element, mode field always present, template analysis compiled only with `templates`
  (plain highlighting otherwise).
- **Explanation:** every building block is designed elsewhere (tokens, live analysis, saving,
  element); this design composes them and adds the preview. Phases 3-4 are written under the
  recommendations, and cannot start before their prerequisites.
- **Open questions:**
  1. **Open design - element shape:** as the leading issue.
  2. **Proposed resolution - command:** `template/editor(state, context)` (namespace `template`,
     feature `templates`) builds the element like `text_editor` (`ui-asset-snapshot-boundary`
     decision 2) in template mode.
  3. **Open design - preview of unsaved text:** this design previews the **saved** template (live
     through dependency invalidation, AC-5). Previewing the buffer needs a way to evaluate a query
     on an in-memory input and deliver the snapshot to an element (`ApplyToInput` delivers
     nothing back). Recommended: saved only now; a follow-up if wanted.
  4. **Proposed resolution - preview channel:** the editor requests snapshots for the preview
     query on its own handle; it already receives no other snapshots, so one monitored asset per
     editor suffices.

## Design Dependencies

- `ui-asset-snapshot-boundary`: **requires** — `TextEditorElement` and command-time loading.
- `text-editor-save`: **requires** — saving (AC-4) and thus the preview refresh (AC-5).
- `template-syntax-highlighter`: **requires** — `TemplateSyntaxToken`, `template_layout_job`.
- `query-editor-live-validation`: **requires** — `UIContext::command_registry`, `EditorDiagnostic`,
  `edit_query_live` (AC-8).
- `console-mounts-nested-element`: **overlaps** — an editor shown through a console needs a handle
  for its preview; without it the preview is absent.
- Overlap judged with `text-editor-save` (T3, same element): kept separate under exclusion **E2**
  (the merge with its prerequisites would be XL); recorded as `requires`.

## Consolidated Findings

- The issue is two requests: the query half is met by `query-editor-live-validation`'s reusable
  `edit_query_live` (AC-8); this design owns the template half.
- Order: `ui-asset-snapshot-boundary` → `text-editor-save`; `query-editor-live-validation` Step 1-3
  → `template-syntax-highlighter`; then this design.
- The preview needs a handle: it works for nodes and, after `console-mounts-nested-element`, for
  editors shown through a console.
