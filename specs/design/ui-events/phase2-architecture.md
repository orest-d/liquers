# Phase 2: Solution & Architecture - ui-events

## Overview

A user action becomes a `UIEvent` (a kind plus the owning element's declared field values),
sent as `AppMessage::UIEvent` and delivered by `AppRunner` as `UpdateMessage::UI` through the
take-update-put path used for snapshots. The web driver finds nodes by markup (`data-lq-event`,
`data-lq-field`, `data-lq-shortcut`) and routes `keydown` through one pure, natively tested
function: a field's Enter action, then menu shortcuts, then the browser. The console handles
`Submit` itself (W1, W2); `UISpecElement` emits its shortcuts (W5).

**Rejected alternatives.**
- *Move `data-lq-action` onto the input or toolbar (W1 only).* A click into the input would
  submit, and W2 remains.
- *Fix W2 inside `lui/submit`.* The command only has `dyn UIElement`; it would need downcasting or a
  console-specific `Custom` payload, a private version of this contract.
- *Carry the query in `AssetSnapshot`.* Fixes refresh only; history and the input stay wrong.
- *One `UpdateMessage` variant per kind.* Viable, but a new arm per kind in every element
  (Phase 1 leading question).
- *A trait method declaring fields.* Invisible to SSR; markup already is the declaration.

## Known-Issue Preflight

| Item | Relation | Blocks? |
|---|---|---|
| the three sources | all reproduced at HEAD | n/a |
| `UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION` (P2, L, no design) | will want per-keystroke field events (a non-goal here) | no |
| `value-accessor` (design, abandoned) | binding layer beneath fields | no |

No open design edits these symbols. Namespaces involved: `lui` (unchanged).

## Data Structures

New module `liquers-lib/src/ui/event.rs`, re-exported from `ui/mod.rs`:

```rust
/// What the user did, independent of backend. Closed: a new kind is a deliberate change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UIEventKind {
    /// The control's default/submit action: Enter in a field, a submit button.
    Submit,
    /// An element-defined event named in markup (`data-lq-event="refresh"`).
    Named(String),
}

/// A user action addressed to one element, carrying its declared fields' live values.
#[derive(Clone, Debug)]
pub struct UIEvent {
    pub kind: UIEventKind,
    /// `(field name, value)` in document order; text fields arrive as text `Value`s.
    pub fields: Vec<(String, Value)>,
}
```

Changed enums (one variant each):

```rust
// element.rs
pub enum UpdateMessage { /* existing four */ UI(super::event::UIEvent) }
// message.rs
pub enum AppMessage { /* existing */ UIEvent { handle: UIHandle, event: UIEvent } }
```

Ownership: `UIEvent` is moved through the channel; `Value` is already `Clone + Send` (it travels
in `AssetSnapshot`), so `AppMessage` keeps `Debug + Clone`. No serde: the event is in-process.

## Trait Implementations

No new trait or method. Each exhaustive `UIElement::update` gains `UpdateMessage::UI(_)`,
answering `Unchanged` except in `QueryConsoleElement` (no `_ =>` arms).

## Sync vs Async

`UIElement::update` stays sync (async work is delegated by message); private
`AppRunner::deliver_update` is async because it locks `AppState` around take/put, like
`deliver_snapshot`; the web routing, `route_keydown` and `from_key_event` are sync.

## Function Signatures

```rust
// event.rs
impl UIEventKind {
    /// `"submit"` -> Submit; other non-empty -> Named; empty -> None.
    pub fn from_name(name: &str) -> Option<Self>;
    pub fn name(&self) -> &str;
}
impl UIEvent {
    pub fn new(kind: UIEventKind) -> Self;
    pub fn with_field(self, name: impl Into<String>, value: Value) -> Self;
    pub fn field(&self, name: &str) -> Option<&Value>;
}
// ui_context.rs
impl UIContext { pub fn send_ui_event(&self, handle: UIHandle, event: UIEvent); }
// shortcuts.rs (all targets): None for a bare modifier or an unknown key.
impl KeyboardShortcut {
    pub fn from_key_event(key: &str, code: &str, command: bool, alt: bool, shift: bool)
        -> Option<Self>;
}
// web/html.rs
pub fn field_attr(name: &str) -> String;            // ` data-lq-field="…"`, escaped
pub fn event_attr(kind: &UIEventKind) -> String;  // ` data-lq-event="…"`
pub fn shortcut_attr(shortcut: &str) -> String;     // ` data-lq-shortcut="…"`
// web/keys.rs (new, webui, all targets; crate-private)
pub(crate) struct KeyPress<'a> { pub key: &'a str, pub code: &'a str, pub command: bool,
    pub alt: bool, pub shift: bool, pub composing: bool }
pub(crate) struct KeyTarget<'a> { pub editable: bool, pub single_line: bool,
    pub event: Option<&'a str> }
pub(crate) enum KeyRoute { FieldEvent(UIEventKind), Shortcut(KeyboardShortcut), Native }
pub(crate) fn route_keydown(press: &KeyPress<'_>, target: &KeyTarget<'_>) -> KeyRoute;
```

`route_keydown` rules, in order: `composing` -> `Native`; Enter with no modifier on a single-line
editable target whose `event` parses -> `FieldEvent`; Tab -> `Native`; editable target and neither
`command` nor `alt` -> `Native`; `from_key_event` yields a shortcut -> `Shortcut`; else `Native`.
`command` is `ctrlKey || metaKey`, matching the semantic `Modifiers::ctrl`.

## Integration Points

- `ui/runner.rs`: generalise private `deliver_snapshot` into
  `deliver_update(handle, message: UpdateMessage, app_state, sender) -> DeliveryOutcome`; the
  snapshot callers pass `UpdateMessage::AssetUpdate(snapshot)`. `process_messages` gains the
  `AppMessage::UIEvent` arm calling it; a `Missing` outcome is ignored.
- `ui/web/app.rs` (wasm `browser` module): split `dispatch_dom_event` into `dispatch_click` and
  `dispatch_keydown(ev, ctx, root)`. Click: nearest `[data-lq-event],[data-lq-action]`; an
  editable node (`INPUT`, `TEXTAREA`, `SELECT`) is ignored; `data-lq-event` goes to
  `send_field_event`, `data-lq-action` to the existing `UiAction` path (factored as `run_action`,
  keeping the `Apply` branch). Keydown: build `KeyPress`/`KeyTarget`, call `route_keydown`;
  `Shortcut(s)` scans `root.query_selector_all("[data-lq-shortcut]")` and runs the first node whose
  parsed shortcut equals `s`. `prevent_default` only when something was dispatched.
  `send_field_event(node, kind, ctx)`: owner = `nearest_element_handle(node)`; collect
  `[data-lq-field]` under `#ui-element-{owner}` whose own nearest handle is the owner; read
  `HtmlInputElement` / `HtmlTextAreaElement` values as `Value::from(String)`.
- `liquers-lib/Cargo.toml`: `web-sys` features `NodeList`, `HtmlTextAreaElement`.
- `widgets/query_console_element.rs`: `render_web` puts `data-lq-field="query"` and
  `data-lq-event="submit"` on the input and renders Go as
  `<button type="button" class="lq-go" data-lq-event="submit">`; the `Apply` action is dropped.
  egui's Enter and Go call the same private `handle_ui_event` inline instead of `submit_query`.
  On `UI(Submit)`, `update` sets `query_text` from field `query` (kept if absent), calls
  `submit_query` and answers `NeedsRepaint`; `Named(_)` answers `Unchanged`.
- `widgets/ui_spec_element.rs`: `render_top_level_item_web` / `render_menu_item_web` add
  `shortcut_attr` for a `Some(shortcut)` on buttons (submenus carry none).

## Documentation Architecture

`specs/reference/UI_INTERFACE_FSD.md` §Event Handling gains the vocabulary, the `data-lq-*`
attributes, keyboard precedence and the reserved-shortcut limit (History row, `reviewed:` bump).
`affects_docs: [UI_INTERFACE_FSD]`.

## Relevant Commands

### New Commands

None.

### Relevant Existing Namespaces

`lui`: `submit` and `query_console` unchanged; `specs/command_registry.yaml` does not change.

## Error Handling

No new `Error` values: an unreadable field is omitted; an unparsable `data-lq-event` or
shortcut, and an event for a removed element, are ignored, as `dispatch_dom_event` treats malformed
actions today. Evaluation errors still reach the console through `AssetSnapshot.error`.

## Risks

| Area | Risk | Mitigation | Certainty |
|---|---|---|---|
| Existing tests | exhaustive matches in `tests/ui_invalidation.rs`, `tests/ui_runner.rs`; the "known gaps" Playwright cases fail by design | updated in the same step | high |
| Compatibility | public enums gain variants; exhaustive matches break | intended; nothing serialized; `liquers-py` unaffected | high |
| Behaviour | keydown Enter no longer dispatches `data-lq-action` | native Enter-to-click on buttons does, once | medium |
| Browser | Chrome never delivers Ctrl+N/T/W to pages, so the example's `Ctrl+N` cannot work there | documented; Playwright uses a deliverable shortcut | medium |
| Concurrency | event and the request it causes share one FIFO channel | drained in order in one `run()` | high |
| Performance | shortcut scan per modified keydown | few nodes; only with `command`/`alt` | high |
| Security | field text becomes a query the console evaluates | unchanged: the same text is evaluated via `lui/submit` today | high |
| Features | `keys.rs` and helpers need `cfg(feature = "webui")`, driver `wasm32` | `scripts/check-build-matrix.sh` | high |
| Recovery | revert; no data or format migration | — | high |
