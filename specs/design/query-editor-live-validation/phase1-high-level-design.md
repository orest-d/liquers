# Phase 1: High-Level Design — query-editor-live-validation

## Purpose

A user typing a query should see, before submitting it, whether it parses and plans: syntax colour
that follows the text exactly, and the first parse or planning error underlined at its position
with its message. This serves the interactive-exploration use case in `specs/reference/VISION.md`
(queries are the user interface), in the egui console first and in the browser console's
rendering second.

## Problem Example

Verified at HEAD (`8ecfd7f`):

- `liquers-lib/src/ui/widgets/query_console_element.rs` `QueryConsoleElement::show_toolbar` builds
  its `TextEdit` layouter from `query_to_layout_job_with_position(&qt, error_position)`, where
  `error_position` comes only from `self.error`, i.e. from an `AssetSnapshot` after submission.
  Nothing validates the text being typed; `UIContext` has no access to the command metadata
  registry.
- The layouter lays out `qt`, a copy taken before the `TextEdit` runs, not the `TextBuffer` egui
  passes it, and it lays out the tokens of the **re-encoded** query (`StyledQuery::from(query)`).
  Typing `ns-pl/-/head` (which parses as resource `ns-pl`, encoded `-R/ns-pl/-/head`, checked with
  `liquers-validate`) produces a galley whose text is not the buffer's.
- The browser console (`render_web`) writes a plain escaped `<input>`; `ui/web/widgets.rs`
  `query_to_html` wraps a query in `<code>` with no colour.

Concrete case: the user types `ns-pl/head-x`. Today the field shows it coloured and silent; Enter
evaluates it and only then the error appears. It should show, while typing, the parameter `x`
underlined and `Can't convert 'x' to integer` (the error `liquers-validate` reports for this query,
`ConversionError` at byte 11), with no evaluation. Typing `hello/q/ns-lui/query_c` should underline
`query_c` (`ActionNotRegistered` at byte 15). Typing `hello/` (incomplete) should show the text
uncoloured with a parse diagnostic, and never lose characters or the cursor.

## Scope and Acceptance Criteria

- **AC-1** Planning error while typing
  WHEN the egui console's text is `ns-pl/head-x` and the context carries the command registry
  THEN the analysis has a diagnostic at bytes 11..12 with the conversion message, and the field
  underlines `x`, without any `AppMessage` being sent
- **AC-2** Unknown command
  WHEN the text is `hello/q/ns-lui/query_c`
  THEN the diagnostic covers `query_c` and says it is not registered
- **AC-3** Tokens follow the source
  WHEN any text is analysed, valid or not
  THEN the tokens cover the text exactly, in order, with no gap or overlap, so their concatenation
  is the text (including `ns-pl/-/head` and link parameters `~X~…~E`)
- **AC-4** Unparseable input keeps the plain path
  WHEN the text does not parse (`hello/`)
  THEN it is one plain token plus a parse diagnostic at the parser's position, and no plan is built
- **AC-5** No registry, parse-level only
  WHEN the context has no registry
  THEN analysis is parse-level: syntax tokens and parse errors only, no planning diagnostic
- **AC-6** Execution errors still shown
  WHEN the text is valid but its evaluation failed (a snapshot error) and the text is unchanged
  THEN that error's position is still underlined, as today; once the text changes, the live
  analysis replaces it
- **AC-7** Browser rendering
  WHEN the browser console renders
  THEN its query appears as token-classed spans (`lq-tok-<kind>`) with the diagnostic span marked
  `lq-tok-error` and its message in a `title`, and `query_to_html` colours queries the same way
- **AC-8** Analysis is cached
  WHEN the egui field is redrawn with unchanged text
  THEN the plan is not rebuilt

Non-goals: live per-keystroke analysis inside the browser (needs per-keystroke field events, a
`ui-events` non-goal; follow-up issue `WEB-QUERY-EDITOR-NOT-LIVE`); completion or suggestions;
multi-line query editing; templates (`template-syntax-highlighter`, `template-editor-ui`).

## Core Interactions and Crate Placement

Query (`liquers-core/src/query.rs`, beside `StyledQueryToken`, so `liquers-template` can reuse it):
source-faithful syntax tokens. Validation: `validate_query`, unchanged. UI (`liquers-lib/src/ui`):
a backend-free `QueryEditorModel`, the registry in `UIContext`, egui and HTML adapters. No new
dependency.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` with "Query editing" (model, registry
  in `UIContext`); `specs/reference/api/DOC_02_QUERY_LANGUAGE_REFERENCE.md` gains the syntax-token
  function next to the styled renderer.
- Guide: none.
- Other documents: the follow-up issue `WEB-QUERY-EDITOR-NOT-LIVE`.
- Documents to update: the source issue (closed on completion).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — L; adds `pub` types in `liquers-core` and `liquers-lib`
  and a `UIContext` builder (rules 1, 3, 4).
- **Leading issue:** **Open design - token model:** a new source-span token
  (`SyntaxToken { kind, start, end }`) in core, rather than reusing `StyledQueryToken`, whose text
  is re-encoded and cannot back an editor. Recommended: add it; keep `StyledQueryToken` for
  rendering finished queries.
- **Explanation:** both diagnostics and positions already exist (`validate_query`, `Error::position`
  in bytes); the work is a faithful tokenizer, a cached model, the registry hand-off and two
  adapters. Phases 3-4 are written under the recommendations below.
- **Open questions:**
  1. **Open design - token model:** as the leading issue.
  2. **Proposed resolution - registry hand-off:** `UIContext::with_command_registry(Arc<…>)`,
     filled by `AppRunner` (which snapshots the environment's registry once at `new`) and by apps
     for their render context; absent means parse-level analysis (AC-5). A registry registered
     after the runner starts is not seen; acceptable, registries are built once.
  3. **Proposed resolution - one diagnostic:** the editor shows the first error only (parse or
     plan; `validate_query` stops at the first), plus plan warnings as non-underlined notes.
  4. **Proposed resolution - browser scope:** render-time highlighting in this design; live
     per-keystroke analysis is the follow-up issue.

## Design Dependencies

- `template-syntax-highlighter`: **required-by** — reuses `SyntaxKind`/`query_syntax_tokens`.
- `template-editor-ui`: **required-by** — reuses `QueryEditorModel`'s pattern and the registry in
  `UIContext`.
- `ui-events` (in review): **overlaps** — its per-keystroke non-goal is what makes browser live
  analysis a follow-up; both edit `QueryConsoleElement::render_web` (attributes there, token
  markup here): a textual rebase, no order.
- `ui-asset-snapshot-boundary` (in review): **overlaps** — both touch `QueryConsoleElement`; no
  shared contract.
- `query-builder` (in review): **overlaps** — validating builder in core; no shared symbol.

## Consolidated Findings

- The egui layouter is wrong independently of validation (stale copy, re-encoded tokens:
  `ns-pl/-/head`); AC-3 fixes it here (triage case 1), no separate issue.
- `validate_query` is synchronous and already returns byte-positioned parse and plan errors; core
  needs only the tokenizer, checked against the whole core parse corpus.
- The registry reaches non-generic, serialized elements only as `Arc<CommandMetadataRegistry>` in
  `UIContext`.
