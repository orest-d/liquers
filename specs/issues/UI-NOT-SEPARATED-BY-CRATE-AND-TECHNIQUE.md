---
id: UI-NOT-SEPARATED-BY-CRATE-AND-TECHNIQUE
kind: feature
title: The UI is one module of liquers-lib with one element trait for every technique and no client-server protocol
status: draft
priority: P1
complexity: XL
area: [lib/ui, web, axum]
design: ui-redesign
created: 2026-11-17
github:
---
## Problem

The whole UI model lives in `liquers-lib/src/ui/` (with the egui widgets in `liquers-lib/src/egui/`),
and every way of showing it goes through one trait. `UIElement` (`liquers-lib/src/ui/element.rs`)
carries the rendering of every technique as feature-gated methods on the same object:
`show_in_egui` (`egui`), `render_web` and `show_in_web` (`webui`). Adding a technique means adding a
method to that trait and a feature to `liquers-lib`; an element cannot be written for one technique
without the trait knowing about all of them.

"Technique" here means a way of presenting a Liquers UI, not a technology: the web alone has at
least two, **monolithic** (engine, element and DOM in one wasm instance, as `mount_web` and
liquers-web's `mountUI` do today) and **split** (engine and authoritative element state on a server,
rendering in a client). Split has no support at all: there is no client-server protocol, so a
browser, or an egui desktop app, cannot show a UI whose elements run on a `liquers-axum` server
(`WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`, `AXUM-CANNOT-SERVE-A-LIQUERS-UI`).

## Impact

Every new UI capability is tied to `liquers-lib` and its feature matrix, and the web client cannot
be built without linking the element types it renders (and with them `polars`, `image-support`).
The VISION client/server deployment has no UI. Workaround: run the whole engine in the page (wasm),
or use the native egui backend.

## Expected behaviour

A UI redesign, owner's direction (2026-11-17):

- The UI is handled by a **separate crate or crates**, outside `liquers-lib`.
- Each technique is a **separate trait**, possibly linked to a common UI element (identity, title,
  handle, place in the tree). The monolithic web technique has one trait (used by liquers-web); the
  split technique has two, a server-side model and a client-side controller/view; egui and other
  frontends get their own.
- A **clear client-server protocol**, serving mainly the split web UI and reusable by other
  techniques (an egui client connected to a server, possibly several clients on one session).
- The current and new implementations run **in parallel** until the new one matures.

The investigative design `specs/design/ui-split-elements/` already works out a reusable part of
this: the model/view split with typed `Down`/`Up` messages, monolithic mode as in-process delivery,
the WebSocket envelope (JSON header plus 8-byte-aligned binary attachments), revisions and resync,
table pages as `RecordView` / Arrow IPC, multiple asset subscriptions per element, and session
separated from connection. A design is needed (complexity XL).

## Discovery

Owner's decision in the project thread that produced `ui-split-elements` (2026-11-17), after
comparing a mode flag, per-mode methods and a model/view pair on an asset viewer and a large table.
