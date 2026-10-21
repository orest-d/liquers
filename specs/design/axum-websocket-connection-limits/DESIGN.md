---
id: AXUM-WEBSOCKET-CONNECTION-LIMITS
kind: design
title: Assets WebSocket connection cap, keep-alive, idle timeout and write timeout
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [axum, docs]
issues: [AXUM-WEBSOCKET-HARDENING]
affects_docs: [specs/reference/WEB_API_SPECIFICATION.md, specs/guides/WEB_API_GUIDE.md]
created: 2026-10-20
---
# Assets WebSocket connection cap, keep-alive, idle timeout and write timeout

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, nothing
implemented. The source issue was linked to `axum-assets-endpoints` (implemented, PR #73), which
delivered half of the hardening (message size, subscription cap, bounded queue, disconnect
cleanup); this design covers the half its progress note left open.

## Phase 1: High-Level Design

### Purpose

The Assets API WebSocket (`ws/q`, `ws/key`) accepts any number of connections, never notices a
vanished peer, and lets a client that stops reading pin its connection forever. Bound all three,
with stated defaults, so a single-binary data service (VISION: "Server / data service", "Low-ops
settings") survives slow and dead clients.

### Problem Example

```rust
// liquers-axum/src/assets/websocket.rs at 18732dd
async fn writer_task(mut sink: SplitSink<WebSocket, Message>, mut rx: mpsc::Receiver<…>) {
    while let Some(message) = rx.recv().await {
        …
        if sink.send(Message::Text(text.into())).await.is_err() { break; }   // no deadline
    }
```

A client subscribes to `ws/q/make_text/upper`, then stops reading (a suspended laptop, a stuck
consumer). Once the socket's send buffer is full, `sink.send` never completes; the 64-slot queue
fills; `Session::reply` and terminal notifications (`tx.send(..).await`) wait forever; the reader
loop blocks in `handle_text`, and nothing ever ends the connection. A peer that disappeared without
a FIN is likewise held until the OS gives up (hours), and 10 000 such connections are accepted.
Should be: the server pings every 30 s, closes a connection that has sent no frame for 90 s (code
1001), drops one whose frame cannot be written within 30 s, and closes connections beyond 1024 at
once with code 1013 ("try again later").

### Scope and Acceptance Criteria

- **AC-1** Connection cap
  WHEN `max_connections` is `Some(n)` and `n` WebSocket connections of one Assets API router are open
  THEN the next one is closed right after the upgrade with close code 1013 and reason "connection limit reached", and a connection opened after one of the `n` closes is served normally
- **AC-2** Server keep-alive
  WHEN `ping_interval` is `Some(d)` and a connection is open
  THEN the server sends a WebSocket Ping frame every `d`
- **AC-3** Silent peer closed
  WHEN a connection receives no frame of any kind (text, ping, pong) for the idle timeout
  THEN the server sends a Close frame with code 1001 and reason "idle timeout", ends every subscription of the connection without cancelling its evaluations, and frees its connection slot
- **AC-4** Live client kept
  WHEN a client sends no messages but answers the server's pings
  THEN its connection stays open past the idle timeout and still answers a later `ping` action
- **AC-5** Slow consumer dropped
  WHEN one outgoing frame cannot be written within `write_timeout`
  THEN the connection is dropped, its subscriptions end, its slot is freed, and other connections keep receiving notifications
- **AC-6** Limits can be disabled
  WHEN a limit is `None`
  THEN that behaviour is as at HEAD (no cap, no pings, no idle close, no write deadline)
- **AC-7** Existing protocol unchanged
  WHEN the default limits apply
  THEN the existing WebSocket suites (`assets_websocket.rs` AWS01-AWS14b, `argument_overrides_ws.rs`) pass with only their `WebSocketLimits` literals completed

Non-goals: per-IP or per-principal limits (no identity exists; `CORE-SESSION-AND-KEY-ACL`);
rate limiting of client messages; a cap on server-wide subscriptions; the Query API timeout
(already configurable); changes to the notification protocol.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `pub` fields to `WebSocketLimits` and observable close
  behaviour (rule 4: public API and HTTP/WebSocket behaviour).
- **Leading issue:** the defaults (Q1): they change what an existing deployment does.
- **Explanation:** all mechanisms are local to `websocket.rs` and `builder.rs` with tokio timers
  already available; three visible choices remain proposals and none changes the plan's shape.
- **Open questions:**
  1. **Open design - defaults on:** `max_connections = Some(1024)`, `ping_interval = Some(30 s)`,
     `idle_timeout = Some(90 s)`, `write_timeout = Some(30 s)` (recommended: hardened by default,
     as the 64 KiB / 256 limits already are), or every new limit `None` until a deployment opts in.
  2. **Proposed resolution - refusal over the cap:** accept the upgrade and close with 1013
     (recommended: a standard WebSocket signal every client library surfaces, and no new
     HTTP-status mapping), rather than an HTTP 503 before the upgrade, which §3.1's
     `ErrorType`-derived status table cannot express.
  3. **Proposed resolution - idle timeout floor:** the effective idle timeout is
     `max(idle_timeout, 2 × ping_interval)` when both are set (recommended: a misconfigured pair
     cannot close a live, ponging client), stated in §5.8.

### Design Dependencies

- `axum-assets-endpoints` (implemented, PR #73) — origin; frozen. Its AWS12a-AWS13 tests stay.
- `unmanaged-asset-registry` — **overlaps**: edits `Session::subscribe` / `cancel` in the same file;
  disjoint functions, no order.
- `core-tracing-migration` — **overlaps**: replaces the `eprintln!` inside `writer_task`, which this
  design rewrites; whichever lands second carries the other's line.
- `access-policy-at-entry-points` — **overlaps**: per-principal limits would build on its principal;
  not needed here.

### Consolidated Findings

- At HEAD (`websocket.rs` 18732dd): no connection count, no server ping, no read deadline, no write
  deadline; `Session::reply` and terminal notifications use an unbounded `send().await`, so a stuck
  writer stalls the reader too. The issue's remaining list is accurate.
- The write deadline is what bounds every other wait: with it, a full queue drains or the writer ends
  within `write_timeout`, so `reply`'s `send().await` is bounded without changing it.
- Three test sites build `WebSocketLimits` literally; they gain `..Default::default()`.
- `max_connections` counts per built router (one `AssetsApiBuilder::build`), shared by `ws/q` and
  `ws/key`; two routers have two budgets.

## Phase 2: Architecture

### Solution

1. **Limits.** Four fields on `WebSocketLimits`; `Default` per Q1.
2. **Gate.** `AssetsApiBuilder::build` creates one `WebSocketGate` (an `Arc<tokio::sync::Semaphore>`
   or nothing when `max_connections` is `None`) and adds it, like the limits, as an `Extension` on
   the four WebSocket routes. `upgrade` calls `gate.try_acquire()` before `on_upgrade`; with a permit
   the permit moves into `handle_socket` and is dropped when it returns; without one the upgraded
   socket gets `Message::Close(Some(CloseFrame { code: 1013, reason: "connection limit reached" }))`
   and nothing else.
3. **Outgoing channel.** The private queue item becomes `enum Outgoing { Notification(
   NotificationMessage), Close(u16, &'static str) }`, so the reader can ask the writer to close.
4. **Writer.** `writer_task` becomes generic over `S: Sink<Message> + Unpin` (unit-testable) and
   `select!`s on `rx.recv()` and a `tokio::time::interval(ping_interval)` (when set; first tick
   skipped). Every `sink.send(..)` is wrapped in `tokio::time::timeout(write_timeout, ..)`; a timeout
   or an error ends the task. `Outgoing::Close` sends the close frame (also under the deadline) and
   ends it.
5. **Reader.** `handle_socket` `select!`s on `stream.next()` under `timeout(effective_idle, ..)`
   and on the writer's `JoinHandle`. Any inbound frame resets the deadline (the timeout is
   re-armed per read). Idle: `tx.try_send(Outgoing::Close(1001, "idle timeout"))`, then the normal
   teardown; writer finished first (slow consumer or socket error): teardown at once. Teardown is
   today's: `unsubscribe_all`, drop the session, then wait for the writer for at most
   `write_timeout` before aborting it.

Rejected: axum's `max_write_buffer_size` alone (bounds memory, not a flush that never completes);
per-message deadlines in `subscription_task` (the queue is the choke point); TCP keepalive (not
reachable through `axum::serve`, minutes-scale).

Known-issue preflight: no open issue blocks. `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS`
and `CORE-TRACING-MIGRATION` touch the same file (Design Dependencies).

### Changes

`liquers-axum/src/assets/websocket.rs`:

```rust
pub struct WebSocketLimits {
    pub max_message_size: usize,              // unchanged
    pub max_subscriptions: usize,             // unchanged
    /// Open connections per router; beyond it a connection is closed with 1013.
    pub max_connections: Option<usize>,
    /// Interval of server Ping frames.
    pub ping_interval: Option<Duration>,
    /// No inbound frame for this long closes the connection (1001); at least 2 × ping_interval.
    pub idle_timeout: Option<Duration>,
    /// Longest wait for one outgoing frame; beyond it the connection is dropped.
    pub write_timeout: Option<Duration>,
}
#[derive(Clone)] pub(crate) struct WebSocketGate(Option<Arc<tokio::sync::Semaphore>>);
impl WebSocketGate { pub(crate) fn new(limits: &WebSocketLimits) -> Self; }
enum Outgoing { Notification(NotificationMessage), Close(u16, &'static str) }   // private
async fn writer_task<S>(sink: S, rx: mpsc::Receiver<Outgoing>, limits: WebSocketLimits)
where S: Sink<Message> + Unpin;
```

`handle_socket` gains `permit: Option<OwnedSemaphorePermit>`; the four handlers take
`Extension<WebSocketGate>`; `Session.tx` and `subscription_task`'s `tx` carry `Outgoing`
(notifications wrapped). `liquers-axum/src/assets/builder.rs` `build`: one `WebSocketGate::new(&
self.websocket_limits)` layered on the four routes. Async throughout; tokio only (axum is native).
Errors: none surface as `Error` — the outcomes are WebSocket close codes. No commands.

Documents: `WEB_API_SPECIFICATION.md` §5.6 (`with_websocket_limits` defaults), §5.8 Limits (the
four limits, close codes 1013/1001, the idle floor, slot release), Appendix B (drop the
WebSocket bullet); `WEB_API_GUIDE.md` §8 one paragraph on reconnecting after 1013/1001. History
rows and `reviewed:` bumps.

### Risks

| Risk | Containment | Certainty |
|---|---|---|
| Defaults close connections an existing client relied on | Q1; browsers and tungstenite answer pings automatically, so only dead peers hit the idle close | medium |
| Timer tests flaky under load | millisecond limits with ×5 margins; assertions on "eventually closed", not exact times | medium |
| A permit leaked on panic in `handle_socket` | the permit is an owned value dropped on unwind | high |
| Close frame cannot be written to a slow consumer | it is sent under the same deadline; the drop happens anyway | high |

Files: `websocket.rs`, `builder.rs`, two test files (literals), the spec and guide. Recovery: one
commit per step; `None` limits restore HEAD behaviour without a revert.

## Phase 3: Examples and Tests

### Examples

```rust
AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/liquer/api/assets")
    .with_websocket_limits(WebSocketLimits {
        max_connections: Some(200),
        idle_timeout: Some(Duration::from_secs(120)),
        ..Default::default()
    })
    .build()
```

A browser that loses Wi-Fi stops answering pings and is closed with 1001 after 120 s; its
evaluations continue and stay cached. The 201st tab gets close 1013 and retries with backoff.

### Tests

Integration, `liquers-axum/tests/assets_websocket.rs` (multi-thread runtime, limits in ms):

- `aws15_connection_limit_closes_with_1013` (AC-1): `max_connections: Some(1)`; a second connect
  receives Close 1013 with the reason; after the first closes (poll up to 2 s), a third gets
  `Initial` for `make_text`.
- `aws16_server_pings_at_interval` (AC-2): `ping_interval: Some(100 ms)`; a reading client sees at
  least two `Message::Ping` within 500 ms.
- `aws17_silent_client_closed_after_idle_timeout` (AC-3): ping 50 ms, idle 200 ms; the client does
  not poll for 800 ms, then reads a Close 1001 (or the stream end); a new connection under
  `max_connections: Some(1)` is then accepted (slot freed).
- `aws18_ponging_client_survives_idle_timeout` (AC-4): ping 50 ms, idle 200 ms; the client keeps
  reading for 800 ms, then sends `{"action":"ping"}` and receives `Pong`.
- `aws19_limits_none_behave_as_before` (AC-6): every new limit `None`; no Ping within 300 ms, and the
  connection stays open while the client does not poll for 500 ms.

Unit, `websocket.rs` `tests`:

- `writer_ends_when_a_frame_misses_write_timeout` (AC-5): a test `Sink` that is never ready;
  `writer_task` with `write_timeout: Some(50 ms)` returns within 500 ms.
- `writer_sends_close_frame_on_request` (AC-3): a collecting sink receives `Close(1001)` after
  `Outgoing::Close`.
- `effective_idle_is_at_least_twice_the_ping_interval` (AC-4): ping 100 ms, idle 50 ms → 200 ms.
- `default_limits` (AC-7): the four defaults of Q1.

Regression (AC-7): `cargo test -p liquers-axum --test assets_websocket --test argument_overrides_ws`.
AC-5 end to end is left to the unit test: filling a real socket's buffer needs megabytes of
notifications and is timing-dependent.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `websocket.rs` `WebSocketLimits` — four fields and `Default` (Q1); complete the three test
  literals with `..Default::default()` — `cargo test -p liquers-axum --lib --tests`
- [ ] 2. `websocket.rs` `Outgoing`, generic `writer_task` with ping interval and write deadline;
  callers wrap notifications — unit tests `writer_ends_…`, `writer_sends_close_…`
  (`cargo test -p liquers-axum --lib websocket`), then AWS01-AWS14b
- [ ] 3. `websocket.rs` `handle_socket` — select on the read deadline and the writer; idle close
  1001; bounded writer wait — `aws16`, `aws17`, `aws18`, `aws19`,
  `effective_idle_is_at_least_twice_the_ping_interval`
- [ ] 4. `websocket.rs` `WebSocketGate`, handlers take it, permit into `handle_socket`;
  `builder.rs` `build` layers it — `aws15`, `route_presence_for_every_switch_combination`
- [ ] 5. Documents: `WEB_API_SPECIFICATION.md` §5.6, §5.8, Appendix B; `WEB_API_GUIDE.md` §8;
  History rows, `reviewed:`; issue `AXUM-WEBSOCKET-HARDENING` closed with evidence —
  `python3 scripts/docs_index.py --check`

Rollback: one commit per step; `None` limits disable the behaviour.

### Validation

`cargo test -p liquers-axum --lib --tests`; `cargo check -p liquers-axum --examples`
(`websocket_client.rs`); `cargo clippy -p liquers-axum --lib`; no `cfg(feature)` change, so no
build matrix run. `specs/index.csv` regenerated; no command registry change.
