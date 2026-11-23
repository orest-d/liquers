# UI transport spike (S2): one abstraction over WebSocket, Tauri IPC, MQTT and others

A spike run on 2026-11-22, true on that date. It is spike S2 of the UI redesign
([plan](../design/ui-redesign/plan.md) §3 and decision 5): prototype the transport the UI protocol
sits on, an ordered two-way channel of envelope frames with connect, close and reconnect, and
check it over WebSocket (required) and Tauri 2 IPC (`invoke` + `Channel`). MQTT, Web Worker
`postMessage`, stdio and in-process were to get a paper check; MQTT, stdio, in-process and Web
Worker were cheap enough to build and measure too. Sessions, revisions and resync are out of
scope: they sit above the transport and belong to D3.

The code is throwaway and is not in the repository. It is kept in the project files under
`spikes/s2-ui-transport/` (a Rust crate `transport` with a `bench` binary, a JavaScript client in
`web/`, a Tauri 2 app in `tauri-bench/`).

## 1. Verdict

- **One abstraction fits all six transports.** A connection is a bounded sender of opaque frames
  plus a receiver of `Frame | Closed(reason)` events; a connector opens one, an acceptor takes
  them in, and reconnecting is connecting again. Every implementation was reduced to that shape
  with two pump tasks, and the same echo service served all of them unchanged (§2).
- **"Ordered and reliable" does not come for free.** Two of the transports broke it under load:
  Tauri async commands reorder pipelined invokes (about 25% of 2,000 frames out of order), and
  MQTT through a broker with default limits silently dropped QoS 1 frames. The contract can stay
  as decision 5 states it, but those two adapters must enforce it themselves (§4.2, §4.3).
- **Three transports have no real backpressure** (browser WebSocket, Tauri `Channel`, MQTT
  through a broker), so the protocol above needs its own flow control: a credit window or acks
  on revisions. Bounded queues inside the Rust adapters are not enough on their own.
- **Tauri IPC is slower than loopback WebSocket on desktop Linux, but fast enough for a UI.** In
  the same WebKitGTK webview, IPC took about 0.55–0.7 ms per small round trip against 0.25 ms for
  WebSocket. Engine-to-view throughput was 3–6 MB/s for 1 KiB frames (WebSocket 210–260 MB/s)
  and about 140–170 MB/s for 64 KiB–1 MiB frames (WebSocket 600–900 MB/s). IPC is bound by
  message rate, about 2,000–5,000 channel messages per second, so frames must be coalesced per
  tick. What it gives in return is no open port and no token, an immediate start, and no sockets
  for iOS to reclaim (§5).
- **Recommendation for D9 (Tauri shell):** use IPC by default, with a sync send command and
  coalesced engine frames, and offer loopback WebSocket as an option for heavy data on desktop.
  Because the abstraction is shared, offering both costs little.

## 2. The abstraction as prototyped

```rust
pub type Frame = bytes::Bytes;                    // opaque envelope frame

pub enum Incoming { Frame(Frame), Closed(CloseReason) }
pub enum CloseReason { Normal, PeerGone(String), Error(String) }

pub struct TransportInfo { name: &'static str, max_frame: Option<usize>, peer: String }

pub struct Connection {
    pub info: TransportInfo,
    pub tx: FrameSender,                  // clone-able; send() awaits a full queue; close()
    pub rx: mpsc::Receiver<Incoming>,     // frames in order, then exactly one Closed
}

#[async_trait] pub trait Connector: Send + Sync { async fn connect(&self) -> Result<Connection, TransportError>; }
#[async_trait] pub trait Acceptor: Send        { async fn accept(&mut self) -> Result<Connection, TransportError>; }
```

The JavaScript side has the same shape: `connect() → { send(Uint8Array), recv(), drain(), close() }`.

What the spike taught about the shape:

- **Frames are bytes.** Every transport carried binary frames: WebSocket binary messages, MQTT
  payloads, length-prefixed stdio, a raw `invoke` body (`tauri::ipc::Request`) up and
  `InvokeResponseBody::Raw` down (an `ArrayBuffer` in JavaScript), and transferable
  `ArrayBuffer`s for workers. Whether envelopes are JSON or a binary encoding is the protocol's
  choice, not the transport's.
- **`max_frame` must be part of `TransportInfo`.** It is effectively unlimited in-process, 64 MiB
  in our WebSocket config, whatever the broker allows for MQTT (often far below 1 MiB on managed
  or embedded brokers) and unbounded but slow for Tauri. The protocol has to chunk large values
  or fetch them out of band by URL, and it needs to know the limit to choose.
- **A peer can vanish without a close.** `Closed(PeerGone)` arrived from a TCP reset (WebSocket),
  EOF (stdio) or a last will (MQTT, after 1.5 × keep-alive). A Tauri page reload drops the
  `Channel`, and the engine only notices on its next failed send; the adapter must hook page-load
  events to emit `Closed` (not done in the spike). A silently dead mobile network gives WebSocket
  no reset at all. The protocol therefore needs its own ping and must not rely on transport
  liveness.
- **Reconnect is a new connection.** `connect` was cycled 20 times per transport (the
  "connect + echo" column). No transport buffered across connections, and none should: resuming
  is the session layer's job (`resume{session, last_revision}` → delta or resync).
- **A sender that does not read can deadlock.** The first order check sent 10,000 frames before
  reading any echoes and hung in-process (2 × 64 queue slots), because the echo blocked on a full
  queue. WebSocket and stdio only survived it thanks to kernel buffers. A protocol endpoint must
  read and write concurrently, which is the same rule as flow control.

## 3. Setup

A 4-vCPU Linux cloud container (kernel 6.18), Rust 1.97 release builds, tokio 1,
tokio-tungstenite 0.30, rumqttc 0.25.1 (MQTT 3.1.1), mosquitto 2.0.18, Tauri 2.12.7 with
WebKitGTK 2.52.6 under Xvfb (software rendering), and headless Chromium 157 through Playwright.
Every number is from one machine with client and server on loopback. They are good for comparing
transports with each other, not as absolute figures. Tauri 3 is in alpha (`3.0.0-alpha.6` on
crates.io), so D9 should check whether its IPC changed.

Measurements (identical in Rust and JavaScript):
- **connect + echo:** open a connection, one echo, close, repeated (10–20×).
- **order:** 2,000 (JS) or 10,000 (Rust) numbered frames echoed back.
- **RTT:** echo round trip for 64 B and 64 KiB frames.
- **down:** the engine streams n frames of 1 KiB / 64 KiB / 1 MiB.
- **up:** the client streams frames and the engine counts them.

Webviews coarsen `performance.now()` (WebKitGTK to 1 ms, Chromium to 0.1 ms without cross-origin
isolation), so the webview tables give the mean RTT over n trips, with p50 / p99 in brackets as
the clock reports them.

## 4. Results

### 4.1 Rust client ↔ Rust engine

| transport | connect + echo µs p50 / p99 | RTT 64 B µs p50 / p99 | RTT 64 KiB µs | down 1 KiB MB/s | down 64 KiB | down 1 MiB | up 1 KiB | up 64 KiB | up 1 MiB |
|---|---|---|---|---|---|---|---|---|---|
| in-process (channels) | 45 / 120 | 40 / 76 | 34 / 59 | 1,174 | (pointer) | (pointer) | 1,612 | (pointer) | (pointer) |
| WebSocket loopback | 254 / 655 | 95 / 218 | 144 / 276 | 252 | 1,414 | 2,337 | 50 | 1,760 | 2,122 |
| stdio (child process) | 2,099 / 2,662 ¹ | 180 / 307 | 278 / 531 | 708 | 417 | 753 | 486 | 785 | 826 |
| MQTT via mosquitto ² | 688 / 1,354 | 227 / 361 | 361 / 636 | 56 | 1,134 | 1,310 | 67 | 916 | 1,000 |

¹ Includes spawning the child process. ² Broker tuned (`max_queued_messages 0`,
`max_inflight_messages 0`, `set_tcp_nodelay true`); with defaults it loses frames (§4.3).

- **In-process at 40 µs** is the cost of four tokio channel hops across threads, not of any
  encoding. Monolithic builds should not use the frame transport at all and should pass values by
  pointer, as plan §1 already says. The in-process transport is useful for tests and for running
  the protocol in one process.
- **WebSocket up at 1 KiB** (50 MB/s against 252 down) is the client sink flushing every message.
  A `feed` + `flush` batching pump would close the gap.

### 4.2 Webview client ↔ Rust engine (Tauri 2, WebKitGTK, same webview, two runs)

| transport | connect + echo µs | RTT 64 B µs mean | RTT 64 KiB µs mean | down 1 KiB MB/s | down 64 KiB | down 1 MiB | up 1 KiB | up 64 KiB | up 1 MiB | out of order (of 2,000) |
|---|---|---|---|---|---|---|---|---|---|---|
| IPC, pipelined async command | 2,000–3,000 | 536–539 | 1,755–2,105 | 3–6 | 139–140 | 159–171 | 9–11 | 295–395 | 589–819 | **508–543** |
| IPC, awaited async command | 2,000 | 689–727 | 2,040–2,530 | 5–6 | 141–151 | 146–177 | 2–3 | 61–92 | 266–272 | 0 |
| IPC, pipelined sync command | 2,000 | 544–650 | 2,010–2,360 | 4–6 | 124–126 | 131–144 | 8–12 | 234–262 | 664–738 | 0 |
| loopback WebSocket | 1,000 | 252–267 | 790–1,100 | 213–256 | 607–697 | 596–889 | 100–116 | 575–669 | 819–874 | 0 |

Startup: from process start to the first `lui_connect` took 674–1,070 ms, almost all of it
creating the webview under Xvfb. The page script ran 13–45 ms after navigation start, the first
IPC echo came 8–14 ms after that, and a first WebSocket connect + echo took another 14–29 ms.

### 4.3 Browser client ↔ engine (headless Chromium 157)

| transport | connect + echo µs p50 | RTT 64 B µs mean | RTT 64 KiB µs mean | down 1 KiB MB/s | down 64 KiB | down 1 MiB | up 1 KiB | up 64 KiB | up 1 MiB |
|---|---|---|---|---|---|---|---|---|---|
| WebSocket → Rust engine | 11,000 | 288 | 976 | 27 | 145 | 101 | 24 | 154 | 81 |
| Web Worker, transferred buffers | 100 | 60 | 91 | 133 | 4,681 | 2,320 | 110 | 1,107 | 1,079 |
| Web Worker, structured clone | 100 | 57 | 161 | 128 | 6,827 | 3,972 | 110 | 520 | 747 |

Headless Chromium's WebSocket was much slower than WebKitGTK's against the same server (11 ms to
connect, about 100–150 MB/s). This is not explained, and it is probably an effect of the headless
network service in this container. Check it on a real desktop browser before using it as a
number. The worker engine was a JavaScript echo standing in for a wasm engine.

## 5. Per transport

### WebSocket (required)

It fits with nothing extra: TCP gives ordering and reliability, and a reset gives `PeerGone`.
Things to carry into D3:
- **Backpressure in the browser** is only `bufferedAmount`; `send` never blocks. The client
  driver has to poll it, or the protocol needs credits.
- **Silent drops** (mobile networks, sleeping laptops) produce no close. A protocol ping with a
  timeout is needed.
- **A loopback server is open to every local process.** Inside a Tauri shell, that means a random
  port and a per-launch token in the first frame, and an `Origin` check.
- **Set `TCP_NODELAY` on accepted sockets.** The spike does; without it, small echoes stall.

### Tauri 2 IPC (`invoke` + `Channel`)

It fits as an `Acceptor`. `lui_connect(on_frame: Channel)` creates the connection, `lui_send`
carries a raw body with an `x-conn` header, and the Rust side is the same echo service.
Findings:
1. **Async commands reorder.** Each async invocation runs as its own task, so pipelined sends
   arrive out of order (about 25%). There are two fixes. A **sync command** runs on the main
   thread in arrival order, kept 100% order at the same throughput, and has to stay trivial (push
   onto an unbounded queue and return). Awaiting each invoke also keeps order, but it halves
   upstream throughput. Sequence numbers in the adapter would also work. Use the sync command.
2. **A `Channel` is bound by message rate.** In `tauri/src/ipc/channel.rs`, a raw message under
   1,024 bytes becomes an `eval` of a JavaScript array literal of numbers, and anything larger
   costs an `eval` plus a `fetch` round trip through the IPC protocol
   (`MAX_RAW_DIRECT_EXECUTE_THRESHOLD`). That is roughly 0.2–0.5 ms per message, whatever its
   size. The protocol should coalesce one tick's deltas into one frame.
3. **On Android, `invoke` bodies cannot go through the custom protocol.** Android's interception
   API cannot read request bodies, so `scripts/ipc-protocol.js` falls back to the `postMessage`
   interface with a serialized payload. View-to-engine binary frames will be slower there. UI
   events are small, so this is probably fine, but it should be measured on a device.
4. **Benefits over loopback WebSocket:** no listening port and no token, since only this webview
   can call the commands; it is usable at the first line of page script; and on mobile it has no
   socket for the OS to reclaim (§6).
5. **The cost** on desktop Linux is 2–3× the round-trip time and 4–40× lower throughput for small
   frames. Large frames are about 4–6× slower. At 140 MB/s, that is still a 1 MiB image in about
   7 ms.

### MQTT (paper check, then built)

It fits, but the adapter has to supply what the broker does not. The spike's topic layout for one
connection with client id `cid` was `lui/{cid}/up`, `lui/{cid}/down` (QoS 1), and
`lui/{cid}/state`, which holds a retained `online` and an `offline` last will. The engine
subscribes to `lui/+/up` and `lui/+/state` and demultiplexes, so one broker session carries
every UI connection. Findings:
1. **The engine demultiplexer must never block.** The first version awaited a full
   per-connection queue. The shared event loop also sends the engine's own publishes and
   keep-alives, so that deadlocked every connection, and the broker fired the engine's last will.
   The spike fixed it with an unbounded inbox per connection, but the real fix is flow control
   above the transport.
2. **The broker drops frames silently.** With mosquitto's defaults (`max_inflight_messages 20`,
   `max_queued_messages 1000`), a 10,000-frame burst lost 3 to about 2,000 QoS 1 frames on every
   run, and the gaps were only seen through sequence numbers. Unlimited queues fixed it in the
   spike. In general, though, the adapter needs sequence numbers and an acknowledged window (a
   small reliable-channel layer), or the protocol has to treat a gap as "resync". MQTT 5's
   *Receive Maximum* bounds the number of unacknowledged QoS 1 messages in flight. It does not
   stop a broker from dropping messages for a slow subscriber whose queue is full.
3. **Nagle.** rumqttc defaults to `tcp_nodelay = false`, and every echo then took about 40 ms. It
   has to be enabled on both the client and mosquitto (`set_tcp_nodelay true`).
4. **Liveness** comes from the last will, after 1.5 × keep-alive (7.5 s with the spike's 5 s).
5. **Size.** Brokers cap packets, and managed or embedded brokers often cap them well below
   1 MiB. Large values should be fetched out of band by URL, which the protocol needs anyway.
6. **MQTT 5 would simplify the layout.** *Response Topic* and *Correlation Data* fit
   request/response, *Maximum Packet Size* sets `max_frame`, *Session Expiry* matches a
   reattachable UI session, and *Message Expiry* can drop stale deltas. rumqttc has a v5 module.
   For "lqe" (embedded Liquers), MQTT is a good fit for many small devices and dashboards behind
   one broker with per-topic ACLs, but not for bulk data.

### stdio

It fits trivially: frames with a 4-byte big-endian length prefix over the child's
stdin/stdout. EOF means closed and the process exit is the liveness signal. Two rules come with
it: the engine must log only to stderr (already a CLAUDE.md rule), and the writer should flush
only when its queue is empty. It is good for a TUI or a test client in a subprocess.

### In-process

It fits, and it is the test transport. Production monolithic builds should skip encoding and
channels altogether (§4.1).

### Web Worker `postMessage`

It fits: a `MessageChannel` per connection, with port messages as frames. It is the fastest of
the cross-boundary transports measured (60 µs RTT, GB/s for large frames). Transferring buffers
mattered for upstream large frames; structured clone was competitive downstream in Chromium.
Ordering is guaranteed per port. Closing a port gives the other side no event, so the adapter
needs an explicit close frame (as in the spike) or a protocol ping. In this configuration the
engine is wasm (`liquers-web`) in the worker and the view runs on the main thread, which is the
fix the plan has in mind for `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`.

## 6. Mobile background behaviour (paper only: no device in the spike)

- **iOS:** when an app is suspended, the system can reclaim its listening sockets
  ([TN2277, Networking and Multitasking](https://developer.apple.com/library/prerelease/mac/technotes/tn2277/_index.html);
  forum reports of a [localhost server breaking after backgrounding](https://developer.apple.com/forums/thread/85038)
  and of [NWListener/NWConnection being reclaimed](https://developer.apple.com/forums/thread/802375)).
  A loopback WebSocket server inside a Tauri iOS app has to be re-created on every resume, and the
  view has to reconnect, which goes through the session-resume path each time. IPC has no socket.
  The engine and the webview are suspended together and continue on resume, so they need no
  reconnect.
- **Android:** IPC uses `postMessage` for `invoke` (§5). A loopback `ws://` connection may run
  into cleartext-traffic policy and needs checking on a device. Cached-app freezing pauses the
  engine and the view together, as on iOS. This was not verified.
- **Either platform:** a remote engine over WebSocket loses its socket in the background, so
  resume-with-revision (D3) is the normal path on mobile, not an edge case.

Mobile behaviour should be measured on devices in D9's mobile phase.

## 7. What D3 and D9 should take from this

1. Define the transport contract as above: opaque byte frames, ordered and reliable, one
   `Closed` at the end, `max_frame` in `TransportInfo`, and reconnect as a new connection.
   Adapters that cannot provide ordering and reliability natively (Tauri async commands, MQTT)
   must provide it themselves.
2. Put flow control in the protocol (credits, or acks on revisions), and require endpoints to
   read and write concurrently.
3. Add a protocol-level ping with a timeout, and do not rely on transport liveness.
4. Coalesce the deltas of one tick into one frame (this matters most for Tauri).
5. Send large values (images, record pages) by reference and fetch them out of band, or chunk
   them by `max_frame`. MQTT needs this, and it also helps everywhere else.
6. D9: use IPC by default with a sync send command, and a loopback WebSocket with a per-launch
   token as an option. Hook page-load events so a reload closes the connection. Re-check this
   against Tauri 3.
7. The monolithic build bypasses the transport, and the in-process transport stays for tests.
