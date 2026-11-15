---
id: UI-SPLIT-ELEMENTS-PROTOCOL-DISCUSSION
kind: analysis
title: Split UI elements — interface options, protocol and runner support, on two examples
workflow: liquers-project
status: draft
area: [lib/ui, axum, web]
created: 2026-11-15
---
# Split UI elements: interface options, protocol and runner support

Input to Phase 1 open question 2 (mode) and to Phase 2. Written 2026-11-15 for discussion with
Orest; nothing here is decided. Code references are to HEAD of the design branch.

## 1. What exists today

- `UIElement` (`liquers-lib/src/ui/element.rs`) is one object: `init`, `update(&UpdateMessage, &UIContext)
  -> UpdateResponse`, `render_web(&self)`, `show_in_egui`, `get_value`, typetag-serialised. It is
  the monolithic shape.
- `AppRunner` (`ui/runner.rs`) delivers asset data as `UpdateMessage::AssetUpdate(AssetSnapshot)`.
  Its monitoring map is **keyed by handle**: one monitored asset per element
  (`AppMessage::RequestAssetUpdates { handle, query }`). An element that watches two assets cannot
  be expressed.
- Delivery is extract-update-replace (`deliver_update`); `NeedsRepaint` records
  `UIChange::Replaced`, which drives incremental repaint (`webui-fixes`).
- `UpdateMessage::Custom(Box<dyn Any + Send>)` exists but nothing routes it from outside.
- Paging queries already exist and validate: `…/-/ns-pl/from_csv/slice-100-50` (`pl/slice`,
  `rec/slice`).

## 2. The two examples

**E1, asset viewer.** Watches a value query and, say, its log or a second related query; shows
status, progress, the value (text inline, image as bytes) and the log. Up: refresh, cancel,
"show log". Needs several subscriptions per element.

**E2, big table.** Source is a polars DataFrame or a record source. The client only ever holds one
materialised page. Up: `RequestPage { offset, len, sort }`. Down: `Loading`, then
`Page { offset, total, rows }`. Reading a page is async work on the server (a record source may
read chunks), and a newer request must win over an older one still running.

## 3. Three interface options

**A. One element, a mode flag.** Today's trait; the element reads `ctx.mode()` and branches:
in `Split` it builds pages and sends them, in `Monolithic` it reads its value directly.

**B. Separate methods per mode, with a default.** A server method and a client method for split
mode, and a fused method for monolithic mode. The only default that can exist is
*monolithic = client ∘ server*: run the server method, hand its output to the client method,
in-process. The opposite default (derive split from a fused method) is impossible, because a
fused method cannot be cut in two automatically.

**C. Model/view pair with a typed message protocol; the transport decides encoding.** B with
its default made the rule: every splittable element is a *model* (server half) and a *view*
(client half) exchanging typed messages. In monolithic mode the messages are delivered in-process
as Rust values, with `Arc<Value>` attachments passed by pointer; in split mode the transport
encodes them. An element may still provide a fused override for monolithic mode when the
exchange is wasteful.

| | A: flag | B: methods + default | C: model/view + typed messages |
|---|---|---|---|
| Code paths per element | two, interleaved by `if` | two or three methods | one protocol; optional fused override |
| Split path exercised by wasm-only tests | no, unless mode is forced | partly (via default) | yes: monolithic runs the same messages |
| Element knows the mode | always must | per method | does not need to; may ask `is_local()` for an optimisation |
| Serialisation in monolithic | avoided by the branch | avoided | avoided: local delivery, values by pointer |
| Wasm client without polars | hard: one type holds both | possible | natural: views and messages are separate types |
| Drift risk between modes | high | medium | low |

**Why C avoids the "data is not sent" problem.** The question "what to send" is answered by the
*view's request*, not by the mode. E2's view asks for 50 rows in both modes, because the DOM
cannot hold 2 million rows either; the model answers with a page in both modes. The modes differ
only in whether that page crosses a wire. Where they genuinely differ in cost (a downscaled image
that monolithic mode does not need), the model may consult `ctx.is_local()`; this is an
optimisation hook, not a second code path.

**Recommendation: C**, with existing `UIElement`s kept as fused, monolithic-first elements (see §6).

**Another option considered, D: synced state.** Model and view share one serialisable
`State` struct; the framework sends the whole struct (later a diff) whenever the model changes
it. Too coarse for E2 (data does not belong in the state) but an excellent *default helper* for
small elements such as E1's status part: `SyncedState<T>` inside option C.

## 4. Signatures (sketch for Phase 2)

```rust
/// Typed messages between the halves. `Serialize` covers the JSON header; value-carrying
/// fields are `Attachment`s, which serialise as indices into the envelope's attachment list.
pub trait UiMessage: Serialize + DeserializeOwned + MaybeSend + 'static {}

pub enum Attachment {
    Local(Arc<Value>),                                   // monolithic: by pointer
    Encoded { data_format: String, media_type: String, bytes: Bytes }, // split: on the wire
}

/// Server half: authoritative state; runs where the engine runs.
pub trait ElementModel: MaybeSend + MaybeSync + Debug {
    type Down: UiMessage;   // model -> view
    type Up: UiMessage;     // view -> model
    fn init(&mut self, ctx: &mut ModelContext<Self::Down>) -> Result<(), Error>;
    fn on_asset(&mut self, sub: SubscriptionId, snapshot: &AssetSnapshot,
                ctx: &mut ModelContext<Self::Down>) -> Result<(), Error>;
    fn on_message(&mut self, msg: Self::Up, ctx: &mut ModelContext<Self::Down>) -> Result<(), Error>;
    /// Full state for first mount and resync (AC-6).
    fn snapshot(&self, ctx: &ModelContext<Self::Down>) -> Result<Self::Down, Error>;
}

/// Client half: renders from its copy; never evaluates queries (AC-8).
pub trait ElementView: MaybeSend + MaybeSync + Debug {
    type Down: UiMessage;
    type Up: UiMessage;
    fn apply(&mut self, msg: Self::Down, ctx: &ViewContext<Self::Up>) -> UpdateResponse;
    fn on_event(&mut self, event: &UIEvent, ctx: &ViewContext<Self::Up>) -> UpdateResponse;
    fn render_web(&self, app_state: &dyn AppState) -> String;
}

impl<D> ModelContext<D> {
    pub fn subscribe(&mut self, query: &str) -> SubscriptionId;   // many per element (E1)
    pub fn unsubscribe(&mut self, sub: SubscriptionId);           // stale page request (E2)
    pub fn send(&mut self, msg: D);        // framework stamps the element revision
    pub fn is_local(&self) -> bool;        // optimisation hook only
}
impl<U> ViewContext<U> { pub fn send(&self, msg: U); }
```

Async work on the server goes through `subscribe`: E2's page is the query
`<source>/-/ns-pl/slice-<offset>-<len>` (or `rec/slice`), so it is evaluated, cached and
cancelled by the asset machinery, and its result arrives in `on_asset`. No element spawns tasks
(consistent with `ui-asset-snapshot-boundary`).

**E2 page format.** Recommended: the page is always a `RecordView`, whatever the source. The model
converts the polars slice (cheap at 50 rows); monolithic passes it by pointer; split encodes it as
Arrow IPC and the client decodes it with `records-ipc`. The view then knows one table type and the
wasm client never links polars.

## 5. Protocol on the wire

One WebSocket per UI session. Each message is the envelope agreed in Phase 1:
`[u32 header length][JSON header][attachments, 8-byte aligned]`, text frame when there are no
attachments.

| Direction | Message | Purpose |
|---|---|---|
| C→S | `hello { protocol, view_types: [{name, version}] }` | handshake; server refuses unknown views |
| S→C | `welcome { session, roots }` | session established |
| S→C | `mount { handle, parent, index, view_type, rev, msg }` | from `UIChange::Inserted`; `msg` is `snapshot()` |
| S→C | `update { handle, rev, msg }` | a model's `send` |
| S→C | `unmount { handle }` | from `UIChange::Removed` |
| S→C | `status { handle, status, error? }` | `RunnerEvent` |
| C→S | `up { handle, base_rev, msg }` | a view's `send` |
| C→S | `resync { handle? }` | gap in `rev`, or reconnect |

Rules: `rev` is per element and strictly increasing; a view drops an `update` whose `rev` is not
greater than its own and requests `resync` on a gap. The server applies `up` regardless of
`base_rev` (it is authoritative) but may use it to drop requests made obsolete. Anything other
than these messages, or an `up` for an unknown handle, closes nothing and evaluates nothing; it is
answered with an error (AC-8).

## 6. Support needed in the UI framework

- **AppState:** a slot per handle holding a model, a view, or both (monolithic), plus its revision.
  A blanket `impl UIElement for Split<M, V>` keeps the existing tree, `render_web` and egui paths
  working. Existing `UIElement`s remain **fused elements**: usable in monolithic mode as today, and
  in split mode only by typetag snapshot, which must refuse a value marked `#[serde(skip)]` (AC-7).
- **Runner:** the monitoring map keyed by `(handle, SubscriptionId)`; delivery of `on_asset` and
  `on_message` with extract-update-replace; an outbox of `Down` messages per tick; the `Up` route
  as a new `AppMessage`; `Inserted`/`Removed` turned into `mount`/`unmount` for a split session.
- **Session (server):** `AppState` + `AppRunner` + transport per connection, over a shared
  environment (lui commands are `payload: required`, so nothing UI-specific is cached across
  sessions).
- **Client (wasm):** a mirror `AppState` holding views, a driver that applies `mount`/`update`/
  `unmount` and feeds the existing `apply_invalidation`, and the existing delegated event listeners
  routing DOM events to `ElementView::on_event`. In monolithic mode the same driver is fed by an
  in-process transport.
- **Registry:** view and message types in a feature-light module that the wasm client links
  without `polars` or `image-support`.

## 7. Decisions for Orest

1. Option C (model/view with typed messages; transport decides encoding; optional fused override)
   instead of a mode flag?
2. Table pages always as `RecordView`, so the client needs `records` but never `polars`?
3. Existing `UIElement`s stay as fused elements, split-capable only by snapshot?
4. Multiple subscriptions per element in the runner (needed by E1), done inside this design?
