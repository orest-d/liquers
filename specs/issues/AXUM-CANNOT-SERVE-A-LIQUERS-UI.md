---
id: AXUM-CANNOT-SERVE-A-LIQUERS-UI
kind: feature
title: A Liquers UI cannot be served or server-rendered by liquers-axum
status: draft
priority: P3
complexity: L
area: [axum, lib/ui]
design: 
created: 2026-11-10
github:
---
## Problem

`liquers-lib` has a server-side renderer, `render_app_ssr` (`liquers-lib/src/ui/web/app.rs`), that
turns an `AppState` into an HTML fragment on any target, and `liquers-lib/tests/webui_ssr.rs`
proves it natively. Nothing serves that HTML. `liquers-axum` depends on `liquers-core` and
`liquers-store` only (`liquers-axum/Cargo.toml`), is generic over `E: Environment`, and its
builders (`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder`, `RecipesApiBuilder`) expose
data endpoints only. There is no route that:

- renders a UI spec or a query's `UIElement` value through `render_app_ssr` into a page;
- serves a trunk-built wasm bundle (`liquers-lib/examples-web/*`) and the stylesheet the web
  backend expects (`examples-web/shared.css` is linked by each example's `index.html` through trunk, not published by any crate);
- makes the SSR output interactive. The fragment keeps its `data-lq-action` / `data-lq-event`
  attributes "for a future hydration script", but `mount_web` always repaints its root from the
  `AppState` it is given rather than adopting existing markup, and there is no script that posts
  those actions back to the server.

The SSR output is therefore a static snapshot that an application has to wrap, serve and refresh
itself.

## Impact

Anyone who wants a server-rendered Liquers page (a dashboard without wasm, a crawlable report, a
fast first paint before the wasm app loads) writes the axum glue by hand, including a page shell,
CSS and a choice of `Environment` whose `Value` is `liquers_lib::value::Value` (which liquers-axum's
handlers do not assume). Workaround: a custom axum handler that builds a `DirectAppState`, runs
`AppRunner` until idle, calls `render_app_ssr` and returns `Html(...)`; and `tower_http::ServeDir`
for a trunk `dist/`.

## Expected behaviour

Some way to mount a UI in an axum router. Options, not chosen here:

1. An optional `ui` feature of `liquers-axum` (pulling `liquers-lib/webui`) with a builder that
   renders a UI spec or query to a full page via `render_app_ssr`.
2. A small helper in `liquers-lib` (behind `webui`) that produces a full HTML document — shell,
   styles, optional `<script>` for a wasm bundle — leaving routing to the application.
3. Hydration: `mount_web` adopting server-rendered markup in its root instead of repainting, so an
   SSR first paint can hand over to the in-browser engine.

Interactivity with the engine kept on the server is a separate problem,
`WEB-UI-CANNOT-BE-DRIVEN-BY-A-REMOTE-ENGINE`. A design folder is needed (complexity L).

## Discovery

Research question in a project thread (2026-11-10): "can the web UI be served from axum (SSR) or
Tauri?". Read `liquers-lib/src/ui/web/`, `liquers-axum/src/*/builder.rs` and
`specs/design/webui/phase2-architecture.md`, which records axum wiring as out of scope.
