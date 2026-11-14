# Phase 1: High-Level Design — Split UI elements

## Purpose

Let one Liquers UI run in two deployments with the same element code: **monolithic**, with the
engine, the element and its rendering in one process (wasm in a browser today), and
**client-server**, with the engine and the authoritative element state on a server (`liquers-axum`,
or the native core of a desktop shell) and rendering in a browser client. This is the "Client/server"
row of `specs/reference/VISION.md` (the server holds the store and heavy commands) applied to the UI,
without rewriting a UI per host (VISION problem 5, platform duplication).

## Problem Example

A dashboard shows a table and a chart:

```
-R/data/sales.csv/-/ns-pl/from_csv      (a polars DataFrame, 2 million rows)
-R/data/chart.png                        (an image)
```

**Today.** The UI only works when the whole engine runs where the DOM is. On wasm, `mount_web`
(`liquers-lib/src/ui/web/app.rs`) holds the `AppState`, `AppRunner` and environment, so the CSV,
polars and every command must run in the tab. On a server, `render_app_ssr` produces a static
fragment that nothing can make interactive (`AXUM-CANNOT-SERVE-A-LIQUERS-UI`). The value view
renders the DataFrame as the first 100 rows of polars' `Display` text
(`ui/web/dataframe.rs` `dataframe_to_html`), with no paging, and an image as a base64 data URI
re-encoded on every render (`ui/web/html.rs` `image_to_html`). An element cannot be serialized with
its value: `AssetViewElement` marks the value `#[serde(skip)]`, so serializing it silently drops
the data.

**Wanted.** In client-server mode the server keeps the DataFrame. The element's server half sends
the client half a status in JSON and one page of rows as an Arrow IPC attachment; "next page" is an
event the client sends up, and the server answers with the next page. The image arrives once, as
PNG bytes. In monolithic mode the same element code runs, the client half reads the DataFrame and
the image directly, and nothing is encoded or decoded.

## Scope and Acceptance Criteria

- **AC-1** DataFrame page over the wire
  WHEN a client-server UI shows `-R/data/sales.csv/-/ns-pl/from_csv`
  THEN the client receives the column names, the total row count and the first page of rows, and
  never the whole DataFrame
- **AC-2** Paging is an event, answered by the server
  WHEN the user activates "next page" in the client
  THEN the client sends an event to that element, and the server answers with the next page and a
  higher revision of the element's state
- **AC-3** An image arrives as bytes
  WHEN a client-server UI shows `-R/data/chart.png`
  THEN the client receives the PNG as a binary attachment of one message, not as base64 text
- **AC-4** Monolithic mode encodes nothing
  WHEN the same UI runs monolithically (wasm, no server)
  THEN the table and image views render from the values themselves, and no value is serialized or
  deserialized for display
- **AC-5** One element definition
  WHEN an element type is used in both modes
  THEN it is the same Rust type with the same update logic; only the link between its halves
  differs
- **AC-6** The server is authoritative
  WHEN the client sends an event based on an older revision, or reconnects after missing messages
  THEN the server's state wins, and the client is brought back to it by a full resync of the
  element
- **AC-7** A value that cannot travel fails loudly
  WHEN an element's state would be sent with a value that has no wire form and the element supplies
  no projection for it
  THEN the client shows an error naming the element and the value type, never an empty view
- **AC-8** Client messages are events, not queries
  WHEN a client sends anything other than an event or action addressed to an existing element
  THEN the server rejects it, and no query is evaluated on its behalf

Non-goals: HTML patches from the server (rejected in discussion: the client renders); JavaScript-
defined elements (`web-ui-define`) in client-server mode; multi-user hardening, authentication and
per-key access (`CORE-SESSION-AND-KEY-ACL`, `AXUM-WEBSOCKET-HARDENING`), which this design must not
preclude; a Tauri adapter (the transport stays pluggable, WebSocket first); the egui backend, which
stays monolithic.

## Core Interactions

- **UI** (`liquers-lib/src/ui/`): element halves, the update-message type, revisions, the link
  abstraction, a native session that owns `AppState` + `AppRunner`, a mirror `AppState` on the
  client.
- **Value types**: whether a value can travel, and its wire form (PNG bytes, Arrow IPC batches).
- **Web/API**: a WebSocket route in `liquers-axum` carrying the update messages.
- **Records**: Arrow IPC (`records-ipc`) as the table page format on both sides.
- **Assets**: the server half subscribes through `AppRunner` (`ui-asset-snapshot-boundary`), never
  from the element.

## Crate Placement

The element model, messages, envelope codec, link trait and session go in `liquers-lib/src/ui/`
(the model is already there and builds on wasm and native). Client element halves must compile
without `polars` or `image-support`, so a wasm client does not link them. The WebSocket route goes in
`liquers-axum` behind a new optional feature that depends on `liquers-lib`; its default build stays
on `liquers-core` + `liquers-store`. `liquers-web` gains a way to mount a client-mode UI.

## Documentation Intent

- Reference: extend `specs/reference/UI_INTERFACE_FSD.md` with split elements, the update message
  and the link; extend `specs/reference/WEB_API_SPECIFICATION.md` with the UI WebSocket route and
  envelope.
- Guide: extend the UI section of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` only if JavaScript
  mounting changes; otherwise none.
- Other documents: none.
- Documents to update: the two references above.
- API docs: `liquers-lib/src/ui/mod.rs` and `liquers-lib/src/ui/web/mod.rs` (new concepts), the new
  link/session modules, and the `liquers-axum` UI route module.

## Open Questions

1. **Envelope.** Recommended: one WebSocket message per update: a JSON header plus attachments
   padded to 8 bytes (`[header length][JSON][attachment…]`), with text frames for attachment-less
   messages. Alternative: CBOR (`ciborium` is already a `liquers-axum` dependency), rejected
   provisionally because its byte strings are unaligned (Arrow IPC would be copied) and the header
   would stop being plain JSON. Confirm.
2. **Monolithic mode: flag or link.** Recommended: no flag. An attachment is either a local
   `Arc<Value>` or encoded bytes, encoding happens only at the transport boundary, and elements can
   ask `is_local()` to skip building a projection at all. Confirm, or keep an explicit flag.
3. **Client copy granularity.** Recommended: the client holds a *named view* of the value: asset
   key, asset version, view parameters (page, sort). It caches by that name and drops entries when
   the version changes. Is a client-side full copy of a value ever wanted (small values only)?
4. **Where do the two halves live in code?** Recommended: a server-half trait and a client-half
   trait, with a blanket "same struct is both" for elements that need no split. Alternative: one
   trait with optional methods. Decide in Phase 2 unless you have a preference.
5. **Relationship to the parked client/server work.** `remote-store-and-asset-manager` is parked
   until client/server is defined, and realms will each have their own environment. Recommended:
   this design defines only the UI channel and assumes one server environment per session; the
   client environment is the mirror, with no commands of its own. Confirm.
6. **First transport.** WebSocket from `liquers-axum`, single local session first (testable with the
   repository's Playwright setup), then multi-session. Confirm the order.

## Design Dependencies

- **Requires / builds on:** `ui-asset-snapshot-boundary` (the runner owns asset subscriptions and
  pushes `AssetSnapshot`s; the server half must receive data this way), `webui-fixes` (the
  `Invalidation` records, reused to sync the client mirror).
- **Overlaps:** `ui-display-conversion` (which display type a value view converts to; the same rule
  picks a projection here); `remote-store-and-asset-manager` (parked; a different channel, data
  rather than UI).
- **Supersedes in part:** the direction recorded in `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`
  (HTML patches) and the interactive half of `AXUM-CANNOT-SERVE-A-LIQUERS-UI`.

## References

Issues `WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`, `AXUM-CANNOT-SERVE-A-LIQUERS-UI`,
`WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`; designs under Design Dependencies; the two references under
Documentation Intent.
