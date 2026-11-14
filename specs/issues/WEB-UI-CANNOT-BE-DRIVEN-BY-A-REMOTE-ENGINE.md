---
id: WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE
kind: feature
title: A browser UI cannot be driven by an engine running in another process
status: draft
priority: P3
complexity: XL
area: [lib/ui, axum]
design: ui-split-elements
created: 2026-11-10
github:
---
## Problem

The web UI backend assumes the engine runs in the same wasm instance as the DOM. The browser
driver in `liquers-lib/src/ui/web/app.rs` (`mount_web_with`, `spawn_driver_loop`) holds the
`AppState`, the `AppRunner` and the `Environment` in-process: delegated `click` / `keydown`
listeners turn `data-lq-action` and `data-lq-event` attributes into `AppMessage`s on a local
channel, the runner evaluates them, and `apply_invalidation` repaints changed elements from
`render_web`. There is no transport between those halves:

- no serialised form of `AppMessage` / `UIEvent` going in, or of `Invalidation` plus rendered HTML
  coming out, over HTTP, WebSocket or a desktop IPC channel;
- `run_headless` separates the driver from the DOM, but only inside one wasm instance;
- `liquers-axum`'s WebSocket (`assets/websocket.rs`) carries asset notifications, not UI events.

So the engine cannot live on a server (axum, "live SSR" in the style of LiveView/htmx) or in a
native process with the UI in a webview (a Tauri app whose Rust core owns the environment, the
file store, polars and tokio).

## Impact

A desktop or server deployment has two options today, both partial. Run the whole engine as wasm
inside the page or webview, losing native-only parts (`AsyncFileStore`, OpenDAL cloud stores,
multi-threaded tokio, native polars performance) and inheriting `WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS`;
or use the native egui backend instead of the web UI. Workaround for Tauri: wasm engine in the
webview, with data reached through a custom store over Tauri commands or `FetchStore` over HTTP
(read-only, `NO-REMOTE-STORE-OR-ASSET-MANAGER`).

## Expected behaviour

A UI whose element tree and engine run natively while a thin script in the page sends events and
applies HTML patches. Pieces it would need, not chosen here: a serde form of `AppMessage` (the
handles and `UiAction` already serialise), a per-session `AppState` + `AppRunner` on the server
with session lifetime and limits (`AXUM-WEBSOCKET-HARDENING`, `CORE-SESSION-AND-KEY-ACL`), a patch
protocol driven by `Invalidation`, and a transport trait with WebSocket and Tauri-IPC
implementations. Native rendering is already available: `render_web` and `render_app_ssr` compile
off wasm. A design folder is needed (complexity XL).

### Sketch from the follow-up discussion (2026-11-10)

Not a decision, recorded so a design does not start from zero.

- No element overrides `show_in_web`; every element renders through `render_web`, so the whole
  tree is renderable natively today.
- `apply_invalidation` (`ui/web/app.rs`) mixes two things: deciding what to replace/insert/remove
  from `Invalidation` + `AppState`, and touching the DOM. Splitting out a pure planner that emits
  DOM operations (`Replace{handle, html}`, `Insert{parent, handle, index, html}`,
  `Remove{handle}`, `All{html}`) would serve both the wasm driver and a remote client.
- `AppMessage` has no serde form; `UIEvent` fields are `Value`s but are text in practice.
  `AppMessage::Serialize` / `Deserialize { path }` touch the filesystem and must not be reachable
  from the wire.
- The parts reusable by both transports: wire types, a native `UiSession` (AppState + AppRunner +
  environment, factored out of `spawn_driver_loop`, woken by messages instead of a 16 ms poll), the
  patch planner, a dependency-free JavaScript client (delegated listeners, `keys.rs` routing,
  focus capture/restore, patch application, resync on sequence gap), and the SSR first paint.
  Only the transport adapter differs: an axum WebSocket route, or Tauri `invoke` + `Channel`.
- Suggested order: axum with one local session first (testable with the repository's existing
  Playwright setup, no webview toolchain), then a Tauri adapter, then multi-session hardening.

## Discovery

Same research thread as `AXUM-CANNOT-SERVE-A-LIQUERS-UI` (2026-11-10), reading the browser driver
and asking what a Tauri app with a native core would have to supply.
