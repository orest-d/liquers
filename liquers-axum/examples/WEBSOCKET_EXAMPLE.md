# Example: Assets API WebSocket notifications

`websocket_client.rs` runs the real Assets API (`AssetsApiBuilder`) on an OS-assigned port and a
client that exercises its WebSocket endpoints. The protocol is specified in
`specs/reference/WEB_API_SPECIFICATION.md` §5; the design is `specs/design/axum-assets-endpoints/`.

```bash
cargo run -p liquers-axum --example websocket_client
```

## What it shows

1. **A query subscription** on `{assets}/ws/q`: subscribe to `hello/slow_upper` and print every
   notification until `JobFinished`.
2. **A key subscription** on `{assets}/ws/key/notes/a.txt` (the path is subscribed on connect):
   the note is written with `POST key/data`, deleted with `DELETE key/data`, and the subscription
   receives `Removed` and ends.
3. **Ping / pong.**
4. **Entry format negotiation:** the same value from `GET q/entry?format=cbor` and `?format=json`.

Expected output (asset ids, timestamps and sizes vary):

```
1. Query subscription on ws/q
  Initial                  hello/slow_upper             status: Processing
  StatusChanged            hello/slow_upper             status: Dependencies
  ValueProduced            hello/slow_upper             status: Ready
  JobFinished              hello/slow_upper             status: Ready

2. Key subscription on ws/key, ended by a DELETE
  Initial                  notes/a.txt                  status: Source
  JobFinished              notes/a.txt                  status: Source
  Removed                  notes/a.txt                  status: -
  (the subscription has ended; subscribe again to follow a new asset)

3. Ping → "Pong"

4. GET q/entry format negotiation
  cbor  application/cbor     541 bytes
  json  application/json     688 bytes
```

## Endpoints

| Endpoint | Subscriptions address | Requested with |
|---|---|---|
| `{assets}/ws/q`, `{assets}/ws/q/{*query}` | a **query** (`parse_query`), e.g. `make_text/upper` or `-R/notes/a.txt` | `AssetManager::get_asset` |
| `{assets}/ws/key`, `{assets}/ws/key/{*key}` | a **key** (`parse_key`), e.g. `notes/a.txt` — no `-R/` | `AssetManager::get` |

`{assets}` is the builder's base path. `with_websocket_path(p)` moves both endpoints to `p/q` and
`p/key`; `without_websocket()` removes them; `read_only()` does not affect them.

## Client messages

```json
{"action": "subscribe", "query": "make_text/upper"}
{"action": "subscribe", "key": "notes/a.txt"}
{"action": "unsubscribe", "query": "make_text/upper"}
{"action": "unsubscribe_all"}
{"action": "ping"}
```

The address field must match the endpoint: `query` on `ws/q`, `key` on `ws/key`. A message that
does not parse, names the wrong field, addresses a key that is neither stored nor declared by a
recipe, or exceeds the subscription limit is answered with an `Error` message; nothing is dropped
silently.

## Server messages

Every asset notification is a flat JSON object with a `type`, the subscription's `asset_id`,
`query` or `key`, a `timestamp`, and `info` — the asset's `AssetInfo` **re-read after the change**:

```json
{"type": "JobFinished", "asset_id": 12, "query": "hello/slow_upper",
 "timestamp": "2026-09-28T22:15:01.123Z", "info": {"status": "Ready", "...": "..."}}
```

| `type` | Extra fields |
|---|---|
| `Initial` | — (sent once on subscribe) |
| `JobSubmitted`, `JobStarted`, `ValueProduced`, `JobFinished`, `LogMessage`, `Expired` | — |
| `StatusChanged` | `status` |
| `ErrorOccurred` | `error` (`ErrorDetail`) |
| `PrimaryProgressUpdated`, `SecondaryProgressUpdated` | `progress` (`message`, `done`, `total`, `timestamp`, `eta`) |
| `Removed` | — (the asset was removed or replaced) |
| `Pong`, `UnsubscribedAll` | `timestamp` only |
| `Error` | `timestamp`, `error` (`ErrorDetail`) |

The core channel keeps only the latest message, so notifications can be coalesced; the `info`
snapshot always reflects the current state, and `GET q/info` / `GET key/info` remain the reliable
polling fallback.

## Subscription lifecycle

A subscription follows **one asset** and ends with it: after the notification whose `info.status`
is `Error`, `Cancelled`, `Expired` or `Volatile`, or after `Removed`. To keep following the key or
query, subscribe again — that requests a fresh asset.

## Limits

`AssetsApiBuilder::with_websocket_limits(WebSocketLimits { max_message_size, max_subscriptions })`,
defaults 64 KiB and 256. A client message over the size limit closes the connection. Disconnecting
aborts the connection's subscription tasks but does not cancel the evaluations, which other clients
may share.
