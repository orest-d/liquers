---
id: WEB-QUERY-EDITOR-LIVE-PREVIEW
kind: design
title: The browser query console's preview follows the text being typed
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/ui]
issues: [WEB-QUERY-EDITOR-NOT-LIVE]
created: 2026-10-26
---
# The browser query console's preview follows the text being typed

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. This is the browser follow-up of `query-editor-live-validation`,
which is in review and not implemented. Implementation starts after that design's Step 5.

## Phase 1: High-Level Design

### Purpose

The egui query console shows syntax colouring and the first parse or planning error while the user
types (`query-editor-live-validation`). The browser console should do the same, so that a user sees
a mistake before submitting it, in either backend (`VISION.md`: queries are the user interface).

### Problem Example

Verified at HEAD (`cd6d5da`), against the planned state after `query-editor-live-validation`:

- At HEAD, `QueryConsoleElement::render_web` (`ui/widgets/query_console_element.rs`) writes a plain
  `<input id="qc-input-{hid}" class="lq-query-input" value="…">`. No preview exists yet.
- `query-editor-live-validation` (Phase 2) adds a `<div class="lq-qc-preview">` under the input,
  filled at render time by `analysis_to_html(&query_text, &analyse_query(&query_text, registry))`.
- The browser driver (`ui/web/app.rs`, `mount_web`) listens only for `click` and `keydown` on the
  root. `render_web` runs only when the element is invalidated (a snapshot or a submit). So the
  preview describes the last rendered `query_text`, not the keystrokes.

Scenario, in `examples-web/ui_query_console_app`: the console shows `hello`, and the user edits the
input to `hello/q/ns-lui/query_c` without pressing Go. Once the parent design is implemented, the
preview still shows `hello`, coloured and with no error. The error appears only after Go, as
`Action 'query_c' not registered…` at byte 15 (`liquers-validate --command hello`). Instead, after
each keystroke the preview should show the typed text, with `query_c` marked `lq-tok-error` and the
message in its `title`. Focus and caret should stay in the input.

### Scope and Acceptance Criteria

- **AC-1** The preview follows typing
  WHEN the user types `hello/q/ns-lui/query_c` into a browser console input without submitting
  THEN within the same event turn the preview shows that text as token spans, with `query_c` in
  an `lq-tok-error` span whose `title` contains "not registered"
- **AC-2** Corrections clear the error
  WHEN the text is then changed to `hello/q/ns-lui/query_console`
  THEN the preview has no `lq-tok-error` span
- **AC-3** Typing is undisturbed
  WHEN the preview updates
  THEN the input element is not replaced, keeps focus and caret, and no `AppMessage` is sent: the
  console's status and content stay as they were
- **AC-4** Render time and live time agree
  WHEN the console is re-rendered (after Go or a snapshot)
  THEN the preview is the render-time analysis of `query_text`, and the same text typed live
  produces identical preview markup
- **AC-5** The input names its preview
  WHEN `render_web` renders a console
  THEN its input carries `data-lq-preview="qc-preview-{hid}"`, and an element with that id exists
  inside the same console element
- **AC-6** Escaping
  WHEN the typed text contains `<b>`
  THEN the preview shows it as escaped text, not markup

Non-goals: per-keystroke `UserEvent`s or `AppState` field sync (a `ui-events` non-goal); query
completion; an editable highlighted field (a `contenteditable` that colours the input itself); the
egui console, which the parent design covers.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible. It depends on `pub` items that the unimplemented parent design
  introduces, and it is `needs-decision` (rule 5). It adds private wasm listener code and a markup
  attribute only, so it could become eligible once the parent has landed and the route is decided.
- **Leading issue:** **Proposed resolution - route:** a third delegated listener (`input`) in the
  wasm driver calls the pure `analyse_query` / `analysis_to_html` of the parent design and patches
  the preview node's `innerHTML` directly, with no `AppState` round trip. Alternative: a
  per-keystroke `UserEvent` through `ui-events`, which re-renders the element. That costs a lock and
  a markup replacement per keystroke, and depends on focus restore, for a result that changes no
  model state.
- **Explanation:** the parent supplies the analysis, the HTML and the registry. This design adds one
  listener, one attribute and one crate-private helper. Phases 3 and 4 are written under the
  recommended route, and against the parent's interfaces as specified in its Phase 2.
- **Open questions:**
  1. **Proposed resolution - route:** as the leading issue.
  2. **Proposed resolution - no debounce:** analysis is a parse plus a plan on a short string,
     microseconds per keystroke (parent Phase 2 risk table), so the preview is updated on every
     `input` event. Add debounce only if a profile shows a need.

### Design Dependencies

- `query-editor-live-validation` (in review, not implemented): **requires**. It provides
  `analyse_query`, `analysis_to_html`, the `lq-qc-preview` div and `AppRunner::command_registry`.
  Implementation order: after its Step 5.
- `ui-events` (in review): **overlaps**. Both edit `mount_web`'s listeners (it splits
  `dispatch_dom_event` into `dispatch_click` / `dispatch_keydown`) and the console's input markup (it
  adds `data-lq-field` / `data-lq-event`). There is no shared contract: the `input` listener and
  `data-lq-preview` sit beside those. Whichever lands second rebases textually.
- `ui-asset-snapshot-boundary` (in review): **overlaps**. It also touches `QueryConsoleElement`, with
  no shared symbol.

## Phase 2: Architecture

### Solution

1. **Markup** (`QueryConsoleElement::render_web`): the input gains
   `data-lq-preview="qc-preview-{hid}"`, and the parent's preview div gains `id="qc-preview-{hid}"`
   (AC-5). The render-time content of the preview is unchanged.
2. **Shared helper** (`ui/web/html.rs`, `webui`, all targets, crate-private):
   ```rust
   /// The preview markup for `text`: the same function at render time and while typing (AC-4).
   pub(crate) fn query_preview_html(text: &str, registry: Option<&CommandMetadataRegistry>) -> String {
       analysis_to_html(text, &analyse_query(text, registry))
   }
   ```
   `render_web` uses it for the render-time preview (it already has `ctx`'s registry under the
   parent design), so the two paths cannot diverge.
3. **Browser driver** (`ui/web/app.rs`, `browser` module, wasm only):
   - In `mount_web`, construct `AppRunner` before the listeners and take
     `let registry = runner.command_registry();` (the parent's `Arc<CommandMetadataRegistry>`).
   - Add a new private `fn update_live_preview(ev: &web_sys::Event, registry: &CommandMetadataRegistry)`.
     It casts the target to `HtmlInputElement`, reads its `data-lq-preview` (and returns if it has
     none), finds that id in the document, and calls
     `set_inner_html(&query_preview_html(&input.value(), Some(registry)))`. It does not call
     `prevent_default`, send messages or lock `AppState` (AC-1, AC-3, AC-6).
   - Register an `input` closure on `root` beside `click` and `keydown`. `MountHandle` gains a
     private `_input: Closure<dyn FnMut(web_sys::Event)>` field, so that dropping the handle detaches
     it.
4. **Re-render:** unchanged. `replace_element_markup` rebuilds the preview from `query_text` with the
   same helper (AC-4).

Rejected alternatives: the per-keystroke `UserEvent` route (leading issue); a `contenteditable`
highlighted input (caret handling across innerHTML changes is the hard part, and it is out of
scope); exposing a `#[wasm_bindgen]` function for page scripts (no consumer, and it would add to the
JavaScript surface).

### Changes

- `liquers-lib/src/ui/widgets/query_console_element.rs` `render_web`: one attribute and one id.
- `liquers-lib/src/ui/web/html.rs`: `query_preview_html` (`pub(crate)`).
- `liquers-lib/src/ui/web/app.rs`: `update_live_preview`, the `input` closure, `MountHandle._input`,
  and runner construction moved before the listeners.
- No `pub` signature change of its own, no command, no new `web-sys` feature (`Event`,
  `HtmlInputElement` and `Document::get_element_by_id` are already used).
- Errors: none. Analysis never fails (parent contract), and a missing preview node is skipped.
- Documents: `specs/reference/UI_INTERFACE_FSD.md` "Query editing" (added by the parent) gets one
  paragraph on browser live preview and `data-lq-preview` (History row, `reviewed:`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| The parent's interfaces change before landing | This design names only `analyse_query`, `analysis_to_html`, `AppRunner::command_registry` and the preview div; re-check them at Step 1 | medium |
| A rebase conflict with `ui-events` in `mount_web` | The listener is additive; the merge is textual | high |
| A re-render while typing overwrites the input value | This already happens today on any invalidation of the console (the `ui-events` W-cases); the preview then matches the restored value (AC-4) | high |
| Per-keystroke cost on long queries | A pure parse and plan; measure in the Playwright run; debounce if needed | high |
| HTML injection | `analysis_to_html` escapes every token (parent); AC-6 test | high |

Recovery: revert the three files. Nothing is persisted.

## Phase 3: Examples and Tests

### Examples

SSR markup of a console with handle 3 and `query_text = "hello"` contains:
`<input id="qc-input-3" class="lq-query-input" value="hello" data-lq-preview="qc-preview-3"/>` and
`<div id="qc-preview-3" class="lq-qc-preview">…</div>`. Typing `hello/q/ns-lui/query_c` (validated:
`ActionNotRegistered` at byte 15) sets that div to the spans of
`query_preview_html("hello/q/ns-lui/query_c", Some(&registry))`.

### Tests

- `liquers-lib/src/ui/web/html.rs` `mod tests`:
  - `query_preview_html_marks_unregistered_action`: AC-1, with a registry built with `hello` and
    the `lui` commands
  - `query_preview_html_valid_text_has_no_error`: AC-2
  - `query_preview_html_escapes_markup`: AC-6
- `liquers-lib/tests/webui_ssr.rs` `console_input_names_its_preview`, which checks the attribute and the id
  pair: AC-5, AC-4 (the render-time preview equals `query_preview_html(query_text, …)`)
- `liquers-lib/examples-web/tests/ui_query_console_app.spec.ts`, new case "preview follows typing".
  Open a console, `fill` → `type` `/q/ns-lui/query_c` after `hello`, and expect
  `.lq-qc-preview .lq-tok-error` to have a title matching `/not registered/`. Expect the input to be
  focused, with `selectionStart` at the end, and the status text unchanged: AC-1, AC-3. Then type
  the correction and expect no `.lq-tok-error`: AC-2.

Run: `cargo test -p liquers-lib --no-default-features --features webui --lib --tests`;
`cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui`;
Playwright after `cargo clean`:
`cd liquers-lib/examples-web && npx playwright test --project ui_query_console_app`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. Precondition: `query-editor-live-validation` Steps 1-5 merged. Re-check `analyse_query`, `analysis_to_html`, `AppRunner::command_registry` and the preview div against this design. Proof: `cargo test -p liquers-lib --no-default-features --features webui --lib --tests` green on the base
- [ ] 2. `ui/web/html.rs` `query_preview_html` and its three tests; `render_web` uses it. Proof: `cargo test -p liquers-lib --no-default-features --features webui --lib web::html` (AC-1, AC-2, AC-6)
- [ ] 3. `render_web`: `data-lq-preview` and the preview id; the SSR test. Proof: `cargo test -p liquers-lib --no-default-features --features webui --test webui_ssr console_input_names_its_preview` (AC-4, AC-5)
- [ ] 4. `ui/web/app.rs`: `update_live_preview`, the `input` closure, `MountHandle._input`, runner construction first. Proof: `cargo check -p liquers-lib --target wasm32-unknown-unknown --no-default-features --features webui`
- [ ] 5. The Playwright case "preview follows typing". Proof: after `cargo clean`, `npx playwright test --project ui_query_console_app` (AC-1, AC-2, AC-3)
- [ ] 6. `UI_INTERFACE_FSD.md` paragraph (History row, `reviewed:`); `WEB-QUERY-EDITOR-NOT-LIVE` `status: closed`; `python3 scripts/docs_index.py`. Proof: `python3 scripts/docs_index.py --check`

### Validation

`bash scripts/check-build-matrix.sh` (the `webui` rows, native and wasm32);
`cargo test -p liquers-lib --lib --tests`. Review the diff: three source files, one test case in
each of the three test locations, and the documents.
