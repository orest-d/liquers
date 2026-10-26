---
id: ASSET-EVENTS-OVER-WEBSOCKET
kind: design
title: A WebSocket endpoint that forwards the manager event stream by scope
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum]
issues: [ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS]
affects_docs: [reference/WEB_API_SPECIFICATION.md, guides/WEB_API_GUIDE.md]
created: 2026-10-26
---
# A WebSocket endpoint that forwards the manager event stream by scope

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. A follow-up of `external-index-sync` (Phase 4 written, not
implemented), whose in-process `AssetManager::subscribe_asset_events` this design forwards; it
cannot land before that design.

## Phase 1: High-Level Design

### Purpose

Let an observer in another process — an index feeder, a cache warmer, a second Liquers instance
(VISION: server / data service; client/server) — follow everything that happens under a key prefix
on a server, with the same "lossless or told" guarantee the in-process stream gives, instead of
polling `listdir` and `entry` on an interval.

### Problem Example

A search indexer runs beside a Liquers server and wants to re-index `docs/` as it changes.
Checked at HEAD (`cd6d5da`, `liquers-axum/src/assets/websocket.rs`):

- `ws/key/docs/a.txt` follows *one* asset and **requests** it (`AssetManager::get`), so following
  a folder means subscribing to every key in it, evaluating each, and missing keys created later;
  `ws/key/docs` subscribes to the directory asset, which never reports its members' changes.
- The per-asset channel is a `watch`, so an `Expired` followed quickly by a recomputation reaches
  the client as `JobFinished` alone. The 64-slot outgoing queue silently drops intermediate
  notifications.
- Once `external-index-sync` lands, `subscribe_asset_events(Under(docs))` exists, but only inside
  the server process.

Should be: the indexer opens `ws/events/docs` and receives
`{"type":"Subscribed","under":"docs"}`, then e.g.
`{"type":"Expired","key":"docs/summary.txt","reason":{"scope":"cascaded",…},"version":"…","live":false}`
for every expiry under `docs/`, and `{"type":"Lagged","count":17}` whenever it fell behind, which
tells it to reconcile.

### Scope and Acceptance Criteria

- **AC-1** Follow a prefix without requesting anything
  WHEN a client connects to `ws/events/docs` (or sends `{"action":"subscribe","under":"docs"}` on `ws/events`) and a write to `docs/a.txt` cascades to `docs/summary.txt`
  THEN it receives `Subscribed`, then an `Expired` message for `docs/summary.txt` carrying its key, reason, version and `live`; no asset is requested or evaluated by the subscription
- **AC-2** Scope
  WHEN events occur under `docs/` and `other/`, and for a non-keyed query
  THEN an `under: "docs"` subscriber receives only the `docs/` ones; a subscriber with no `under` receives all, queries included (as a `query` field)
- **AC-3** Told, never silently lossy
  WHEN the client stops reading while more events occur than the server-side buffers hold, then resumes
  THEN it receives a `Lagged` message with `count` > 0 (the sum of events dropped in the manager's broadcast and in the socket queue) before any later event, and later events arrive in order
- **AC-4** Off unless enabled
  WHEN an Assets API router is built without `with_event_stream(true)`
  THEN `ws/events` is not routed (404), and `ws/q` / `ws/key` behave as today
- **AC-5** One scope per connection
  WHEN a client sends a second `subscribe`, or `unsubscribe`
  THEN the previous scope stops (no further events from it); a new `Subscribed` acknowledges the replacement, `Unsubscribed` the stop
- **AC-6** Protocol errors
  WHEN a client sends an unparsable `under`, an unknown action or a binary frame
  THEN it receives an `Error` message and the connection stays open
- **AC-7** Every event kind is forwarded
  WHEN the core `AssetEvent` enum has a variant (from `asset-event-kinds`: `Updated`, `Removed`)
  THEN it has a message type of the same name; clients are specified to ignore unknown `type` values
- **AC-8** Server shutdown
  WHEN the manager's event channel closes
  THEN the client receives `Closed` and the connection is closed normally

Non-goals: replay or resume from a sequence number after a disconnect (a reconnecting client
reconciles, which is the correctness guarantee anyway); a Rust or JavaScript client library
(`remote-asset-manager` will need one); per-key access filtering before
`access-policy-at-entry-points` exists; HTTP long-polling or SSE.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new route, new `pub` wire types and a builder option
  (rules 3, 4), and the core stream it forwards does not exist yet.
- **Leading issue:** Open design question — whether the endpoint is on by default.
- **Explanation:** the core stream already provides scope, order and an explicit lag signal; the
  server only has to map it to JSON and carry the lag through its own bounded queue. The
  remaining questions are exposure and wire-shape choices with recommended answers.
- **Open questions:**
  1. **Open design - default exposure:** off by default, enabled with
     `AssetsApiBuilder::with_event_stream(true)` (recommended). The stream reveals the activity of
     every client of the server (which keys are evaluated, written, removed, and when), which no
     existing route does; `listdir` reveals names, not activity. Alternative: on with the other
     WebSocket endpoints, like `ws/key`.
  2. **Open design - endpoint shape:** a separate endpoint `ws/events` (recommended: its
     subscriptions request nothing and its messages have another shape, so mixing them into
     `ws/key`'s protocol would make every `ws/key` client handle two message families).
     Alternative: a `subscribe_events` action on `ws/key`.
  3. **Proposed resolution - one scope per connection:** a second `subscribe` replaces the first.
     A client wanting two prefixes opens two connections, which the connection cap of
     `axum-websocket-connection-limits` counts.
  4. **Proposed resolution - wire form:** flat messages with a `type` tag, the subject as `key` or
     `query` (encoded string), `version` as the 32-hex string `Version` already serializes to, and
     `reason` as the `ExpiryReason` serde form already stored in metadata. Clients must ignore
     unknown `type`s, so a new event kind is additive on the wire.
  5. **Implementation details (resolved in Phase 2):** the socket-side drop count is folded into
     the next `Lagged`; the event task uses `try_send` so a slow reader never blocks the manager's
     broadcast (it only lags); `writer_task` becomes generic over the message type.

### Design Dependencies

- `external-index-sync` — **requires**: `AssetManager::subscribe_asset_events`, `AssetEvent`,
  `AssetEventScope`, `AssetEventReceive` (with `Lagged` and `Closed`).
- `asset-event-kinds` — **overlaps**: adds `Updated` / `Removed`; whichever lands second adds the
  wire arms (the exhaustive match in the mapping forces it). Weak overlap: T2 was considered and
  does not hold, because AC-7's "ignore unknown types" rule makes the two independent.
- `axum-websocket-connection-limits` — **overlaps**: its connection cap, ping, idle and write
  timeouts must cover `ws/events` too; both edit `websocket.rs` / `builder.rs` in disjoint
  functions. Finished Phase 4, not extended.
- `access-policy-at-entry-points` — **overlaps**: when it lands, the upgrade captures the principal,
  `subscribe` checks `key(Read)` on the scope, and each forwarded event is checked for its key;
  until then question 1's default is the guard.
- `remote-asset-manager` — **required-by**: a proxy manager invalidates and reconnects through this
  stream.
- `unmanaged-asset-registry` — **overlaps** (same file, `ClientMessage`): disjoint enums.

### Consolidated Findings

- The forwarding is a pure axum change on top of `external-index-sync`: a new file
  `liquers-axum/src/assets/events.rs`, a builder option, two routes, `writer_task` generalised.
  No core change.
- Two buffers can lose events: the manager's broadcast (reported by the core as `Lagged(n)`) and
  the socket's 64-slot queue. Both are reported in one `Lagged { count }`, sent before the next
  event, so AC-3 holds end to end. A blocking `send().await` was rejected: it would let one slow
  client hold its broadcast receiver back until it lags anyway, with the same result and a stuck
  task.
- Key names leave the process: the endpoint is off by default until access policy exists
  (question 1).

## Phase 2: Architecture

### Solution

A new handler family `ws/events`, `ws/events/{*key}` in `liquers-axum/src/assets/events.rs`. Each
connection has one reader loop (client messages), one writer task (the existing `writer_task`,
made generic), and at most one forwarding task that owns an `AssetEventSubscription` and maps each
`AssetEventReceive` to an `EventMessage`.

Rejected: reusing `ws/key` (question 2); unbounded per-connection queue (memory grows with a
stalled client); SSE (the API already standardises on WebSocket and its limits); sequence numbers
for resume (the core stream has none, and reconciliation is required after any gap anyway).

### Changes

`liquers-axum/src/assets/events.rs` (new; `pub` items re-exported from `assets/mod.rs`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EventClientMessage {
    /// Follow `under` (a key, `parse_key`); absent: everything, queries included.
    Subscribe { #[serde(default)] under: Option<String> },
    Unsubscribe,
    Ping,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum EventMessage {
    Subscribed { timestamp: String, #[serde(skip_serializing_if = "Option::is_none")] under: Option<String> },
    Expired {
        timestamp: String,
        #[serde(flatten)] subject: EventSubject,   // {"key": "..."} or {"query": "..."}
        reason: liquers_core::metadata::ExpiryReason,
        #[serde(skip_serializing_if = "Option::is_none")] version: Option<Version>,
        live: bool,
    },
    // Updated { timestamp, subject, version, origin }, Removed { timestamp, subject }:
    // added with asset-event-kinds.
    Lagged { timestamp: String, count: u64 },
    Unsubscribed { timestamp: String },
    Pong { timestamp: String },
    Error { timestamp: String, error: ErrorDetail },
    Closed { timestamp: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSubject { Key(String), Query(String) }  // serialized as a one-field object

pub async fn ws_events_handler<E: Environment>(..) -> Response;       // ws/events
pub async fn ws_events_path_handler<E: Environment>(..) -> Response;  // ws/events/{*key}
fn event_message(event: AssetEvent) -> EventMessage;  // exhaustive match, no `_` arm
```

Forwarding task: a `tokio::select!` loop over two branches. (1) `sub.recv()`: `Event(e) =>
forward(e)`, `Lagged(n) => pending += n`, `Closed => { send Closed; break }`, where `forward` first
tries to flush a pending `Lagged { count: pending }` with `tx.try_send`, then the event; a `Full`
result adds 1 to `pending` (and keeps the `Lagged` pending). (2) Only while `pending > 0`:
`tx.reserve()`, which completes when the socket queue has room; the permit sends
`Lagged { count: pending }` and resets `pending`. Branch (2) makes the loss notice independent of
new source events: after a burst followed by an idle source, the reader still gets `Lagged` as
soon as it drains the queue (Codex review on PR #183). `Closed` from the queue ends the task. A replaced or
unsubscribed scope aborts the task (as `Session::unsubscribe` does today).

`websocket.rs`: `writer_task` generic over `T: Serialize + Send + 'static`; `OUTGOING_QUEUE` and
`WebSocketLimits::max_message_size` reused. No change to `ClientMessage` / `NotificationMessage`.

`builder.rs`: field `event_stream: bool` (default `false`), `pub fn with_event_stream(mut self,
enabled: bool) -> Self`; in `build`, when the WebSocket path is set and `event_stream` is true,
route `{ws}/events` and `{ws}/events/{*key}` with the same `limits` layer. `without_websocket()`
removes them too.

Errors: an invalid `under` is a `ParameterError` `ErrorDetail` (`error_to_detail`), as `ws/key`
does; the path form with an invalid key is answered after the upgrade with `Error`, then the
connection stays open with no scope.

Documents: `WEB_API_SPECIFICATION.md` §5.8 gets a third row and a "Manager event stream"
subsection (messages, `Lagged` contract, unknown-type rule, off by default; History row,
`reviewed:` bump); `WEB_API_GUIDE.md` an indexer example.

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-axum/src/assets/events.rs` (new), `websocket.rs` (`writer_task` generic), `builder.rs`, `mod.rs` |
| Crates and workflows | `liquers-axum` only; existing endpoints untouched |
| Existing tests | none change (`writer_task`'s callers keep their type) |
| New validation | `liquers-axum/tests/assets_events_websocket.rs` (`aev01`-`aev06`), two unit tests in `events.rs` |
| Compatibility | Additive route, off by default; wire is additive by the unknown-type rule |
| Concurrency / performance | One broadcast receiver per connection; `try_send` never blocks the manager. A stalled client costs one 64-slot queue and a counter |
| Security | Activity and key names leave the process when enabled (question 1); per-key filtering waits for access policy |
| Recovery | Disable the option or revert the file |
| Certainty | High, given `external-index-sync`'s interface as written; the test harness of `assets_websocket.rs` is reused |

## Phase 3: Examples and Tests

### Examples

```
GET ws/events/docs                                   (upgrade; scope Under(docs))
← {"type":"Subscribed","timestamp":"…","under":"docs"}
  (server: set_state docs/a.txt; docs/summary.txt's recipe reads -R/docs/a.txt/-/to_text)
← {"type":"Expired","timestamp":"…","key":"docs/summary.txt",
   "reason":{"scope":"cascaded","cause":{"kind":"updated",…},…},"version":"…","live":true}
→ {"action":"subscribe"}                              (everything)
← {"type":"Subscribed","timestamp":"…"}
→ {"action":"subscribe","under":"<a string parse_key refuses>"}
← {"type":"Error","timestamp":"…","error":{"type":"ParameterError",…}}
```

The recipe query `-R/docs/a.txt/-/to_text` validates with `liquers-validate`
(`GetAsset[docs, a.txt]`, `Action{to_text}`).

### Tests

`liquers-axum/tests/assets_events_websocket.rs`, reusing the helpers of `assets_websocket.rs`
(`env_with`, `start_server`, `connect`, `recv_by_type`) with `with_event_stream(true)`:

- `aev01_path_scope_receives_cascaded_expiry` — AC-1 (also asserts no `Initial`/`JobSubmitted`
  and that `docs/summary.txt` was not evaluated by subscribing)
- `aev02_scope_filters_and_all_includes_queries` — AC-2
- `aev03_stalled_reader_gets_lagged_then_ordered_events` — AC-3 (a small
  `ASSET_EVENT_BUFFER` is not configurable, so the test generates more than 1024 + 64 expiries
  with a non-reading client, then reads: first non-`Subscribed` message is `Lagged` with
  `count > 0`, subsequent `Expired` keys in send order)
- `aev04_disabled_by_default` — AC-4 (upgrade on `ws/events` fails with 404; `ws/key` works)
- `aev05_resubscribe_replaces_and_unsubscribe_stops` — AC-5
- `aev06_protocol_errors_keep_connection` — AC-6
- unit test in `events.rs`: `pending_lag_flushes_on_an_idle_source` — AC-3 (a scripted source sends
  a burst that overflows a one-slot queue and then stays idle; once the test drains the queue, a
  `Lagged` arrives with no further source event)
- unit test in `events.rs`: `every_asset_event_has_a_message_type` — AC-7 (constructs each
  `AssetEvent` variant and checks the serialized `type`; the exhaustive match makes a new variant
  a compile error first)
- unit test in `events.rs`: `forward_events_folds_lag_and_ends_on_closed` — AC-3, AC-8. The
  forwarding loop is written as `forward_events(next, tx)` over any source of
  `AssetEventReceive` (the subscription in production), so a scripted source
  (`Event`, `Lagged(3)`, `Event`, `Closed`) and a one-slot queue that is full for one send
  prove the folded `Lagged { count: 4 }`, its position before the next event, and the final
  `Closed`. A live server never drops its manager, so AC-8 is not provable end to end

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-axum --test assets_events_websocket --lib`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-axum/src/assets/websocket.rs` `writer_task` generic over the message type —
  proof: `cargo test -p liquers-axum --test assets_websocket` unchanged and green (requires
  nothing)
- [ ] 2. `liquers-axum/src/assets/events.rs`: `EventClientMessage`, `EventMessage`,
  `EventSubject`, `event_message` with an exhaustive match over `AssetEvent` — proof:
  `every_asset_event_has_a_message_type` (requires `external-index-sync` merged)
- [ ] 3. `events.rs`: session (one scope), `forward_events` with `Lagged` folding, handlers —
  proof: `forward_events_folds_lag_and_ends_on_closed`, `aev01`, `aev02`, `aev03`, `aev05`,
  `aev06`
- [ ] 4. `builder.rs` `event_stream`, `with_event_stream`, routes; `mod.rs` re-exports — proof:
  `aev04`, builder unit test `with_event_stream_routes_events`
- [ ] 5. Documents: `WEB_API_SPECIFICATION.md` §5.8, `WEB_API_GUIDE.md` (History rows, `reviewed:`
  bumps); close `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS` — proof:
  `python3 scripts/docs_index.py --check`

Rollback: steps 2-4 are one new file and an off-by-default option; reverting them leaves step 1,
which is behaviour-neutral.

### Validation

`cargo test -p liquers-axum`; `cargo clippy -p liquers-axum`; `python3 scripts/docs_index.py
--check`. If `axum-websocket-connection-limits` has landed, add `ws/events` to its connection cap
and timeouts in step 3 and extend its tests by one case each. Final diff review: no `println!`, no
`_ =>` over `AssetEvent`, no change to `ws/q` / `ws/key` messages.
