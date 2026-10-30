# Phase 3: Examples & Use-cases - ui-events

## Example Type

Runnable: native unit and integration tests in `liquers-lib`, plus Playwright cases against the
existing `liquers-lib/examples-web/ui_query_console_app` example. The browser driver compiles only
for wasm32, so DOM routing is proven by Playwright and its decision logic by the pure
`route_keydown` tests.

## Overview Table

| # | Type | Name | Proves |
|---|------|------|--------|
| 1 | Example | Enter submits the console | AC-1, AC-2 |
| 2 | Example | Menu shortcut in the browser | AC-5, AC-6 |
| 3 | Example | A custom element reacts to a named event | AC-7 |
| 4 | Tests | Unit, integration and Playwright suites below | AC-1 to AC-7 |

## Example 1: Enter submits the console

Setup: the example app (commands `hello`, `about` and the `lui` namespace); the menu item runs
`hello/q/ns-lui/query_console/add-child` (validated with `liquers-validate --command hello`).
The console renders:

```html
<div id="ui-element-3" class="lq-element lq-QueryConsoleElement"><div class="lq-qc-toolbar">
  <input id="qc-input-3" class="lq-query-input" value="hello" data-lq-field="query" data-lq-event="submit"/>
  <button type="button" class="lq-go" data-lq-event="submit">Go</button>…</div>…</div>
```

The user types `about` and presses Enter. `route_keydown` returns `FieldEvent(Submit)`; the
driver sends `AppMessage::UIEvent { handle: UIHandle(3), event: UIEvent { kind: Submit,
fields: [("query", Value::from("about"))] } }`. The runner delivers `UpdateMessage::UI`; the
console sets `query_text = "about"`, pushes it to history and sends `RequestAssetUpdates { handle:
3, query: "about" }`. The content shows "Liquers query console — browser port" and the
re-rendered input keeps `about`. Clicking Go produces the same event.

## Example 2: Menu shortcut in the browser

The `UISpec` menu button renders as
`<button class="lq-menu-button" data-lq-action='…' data-lq-shortcut="Alt+N">New Console</button>`.
Alt+N anywhere in the page (also inside the query input, because Alt is held) matches it and runs
its `UiAction::Query`, adding a console. Typing `n` in the input matches nothing and only edits
the text. The web example's shortcut changes from `Ctrl+N` to `Alt+N` because Chrome never
delivers Ctrl+N to a page; the egui example keeps `Ctrl+N`.

## Example 3: A custom element reacts to a named event

```rust
fn update(&mut self, message: &UpdateMessage, ctx: &UIContext) -> UpdateResponse {
    match message {
        UpdateMessage::UI(event) => match &event.kind {
            UIEventKind::Named(name) if name == "reset" => { self.count = 0; UpdateResponse::NeedsRepaint }
            UIEventKind::Named(_) | UIEventKind::Submit => UpdateResponse::Unchanged,
        },
        UpdateMessage::AssetNotification(_) | UpdateMessage::AssetUpdate(_)
        | UpdateMessage::Timer { .. } | UpdateMessage::Custom(_) => UpdateResponse::Unchanged,
    }
}
```

Markup `<button type="button" data-lq-event="reset">Reset</button>` is all the element declares.

## Corner Cases

### 1. Memory
Fields are copied once per event; no listener or handle is retained per element, so a re-render
leaks nothing.

### 2. Concurrency
The event and the `RequestAssetUpdates` it triggers share the FIFO channel and are drained in one
`run()`. An event for an element removed meanwhile meets `DeliveryOutcome::Missing` and is dropped.

### 3. Errors
Empty submitted text sets `query_text` to empty and submits nothing (existing `submit_query`
guard). An unknown `data-lq-event` value is a `Named` event the element ignores. A malformed
`data-lq-shortcut` never matches. None raises an error.

### 4. Serialization
Nothing new is serialized. `QueryConsoleElement` serde round-trips keep passing; the submitted
query persists through the existing `query_text` and `history` fields.

### 5. Integration
IME composition, Tab and plain typing stay native (AC-6). Enter on a focused menu `<button>` is
dispatched once by the browser's own click. A nested element's fields are not collected by its
parent: field ownership is the nearest `ui-element-` ancestor.

## Test Plan

### Unit Tests

`liquers-lib/src/ui/event.rs`:
- `ui_event_kind_from_name` (AC-7): `submit`, a custom name, empty.
- `ui_event_field_lookup` (AC-7): `with_field` then `field`, missing name is `None`.

`liquers-lib/src/ui/widgets/query_console_element.rs`:
- `test_user_submit_sets_query_history_and_requests_updates` (AC-1, AC-2): handle set, initial
  `hello`; `update(UI(Submit, query=about))` returns `NeedsRepaint`, `query_text == "about"`,
  history ends with `about`, the channel holds `RequestAssetUpdates { query: "about" }`.
- `test_user_submit_without_field_resubmits_current_query` (AC-2).
- `test_user_named_event_is_unchanged` (AC-7).
- `test_expired_after_user_submit_requests_submitted_query` (AC-3): submit `about`, then an
  `AssetUpdate` with `Status::Expired`; the next request carries `about`.
- `test_volatile_refresh_after_user_submit_uses_submitted_query` (AC-3, `#[tokio::test]`, modelled
  on `test_volatile_asset_schedules_delayed_refresh`).
- `test_render_web_declares_query_field_and_submit_event` (AC-1, AC-4; `cfg(feature = "webui")`):
  the input carries `data-lq-field="query"` and `data-lq-event="submit"`, Go is a
  `<button type="button"` with `data-lq-event="submit"`, and no `data-lq-action` remains.
- `test_render_web_after_user_submit_shows_submitted_query` (AC-2; webui): `value="about"`.

`liquers-lib/src/ui/shortcuts.rs`:
- `from_key_event_matches_parsed_shortcut` (AC-5): `("n", "KeyN", true, false, false)` equals
  `KeyboardShortcut::parse("Ctrl+N")`; `("˜", "KeyN", false, true, false)` equals `Alt+N`.
- `from_key_event_bare_modifier_is_none` (AC-6): `("Control", "ControlLeft", true, …)`.

`liquers-lib/src/ui/web/keys.rs` (webui):
- `enter_in_declared_field_is_field_event` (AC-1).
- `enter_while_composing_is_native` (AC-6); `tab_is_native` (AC-6);
  `plain_key_in_field_is_native` (AC-6); `enter_in_field_without_event_is_native` (AC-6).
- `alt_key_in_field_is_shortcut` (AC-5); `command_key_outside_field_is_shortcut` (AC-5).

`liquers-lib/src/ui/web/html.rs` (webui): `field_event_and_shortcut_attrs_escape` (AC-5, AC-1).

`liquers-lib/src/ui/widgets/ui_spec_element.rs` (webui):
`render_web_emits_shortcut_attribute` (AC-5): a button with `shortcut: Ctrl+S` renders
`data-lq-shortcut="Ctrl+S"`; one without renders none.

### Integration Tests

`liquers-lib/tests/ui_runner.rs`:
- `ui_event_reaches_element_and_records_repaint` (AC-7): a test element records the `UI`
  event and answers `NeedsRepaint`; after `AppMessage::UIEvent` and `run()`, it saw `Submit`
  with the field, and `take_invalidation()` contains `UIChange::Replaced { handle }`.
- `ui_event_for_missing_element_is_ignored` (AC-7): no panic, no invalidation.

`liquers-lib/tests/query_console_integration.rs`:
- `test_ui_submit_event_evaluates_new_query` (AC-1, AC-2, AC-3): console created with `hello`
  plus a second registered command; a `Submit` event with the second query makes the next
  `AssetUpdate` carry its value and leaves `query_text` and history on it.

### Manual Validation

Playwright, `liquers-lib/examples-web/tests/ui_query_console_app.spec.ts`; the "known gaps" block
is replaced:
- `pressing Enter in the input submits` (AC-1) — replaces the W1 case.
- `the submitted query stays in the input` (AC-2) — replaces the W2 case; asserts `about`.
- `clicking into the input does not submit` (AC-4).
- `a menu shortcut adds a console` (AC-5): Alt+N adds one `.lq-QueryConsoleElement`.
- `typing and Tab in the input dispatch nothing` (AC-6): typing `n` and pressing Tab leave the
  console count and content unchanged.

## Documentation and Learning Log

- The "known gaps" Playwright cases were written to fail when this lands; they are rewritten, not
  deleted.
- Chrome-reserved shortcuts are a platform limit; the reference records it (Phase 2).
