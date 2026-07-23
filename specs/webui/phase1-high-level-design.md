# Phase 1: High-Level Design - webui

## Feature Name

webui — web-framework-independent web rendering backend for `liquers_lib::ui`

## Purpose

Add a web rendering backend for the existing `liquers_lib::ui` element tree that
depends only on `web-sys`/`wasm-bindgen` (no Dioxus, Leptos, Yew, or other web
framework). It is the web-native counterpart of the egui reference backend and
works in two modes: **browser** (wasm, `web-sys` DOM, full interactive support)
and **server-side** (SSR, HTML-string generation, limited/non-interactive
support). egui and webui must be independent, mutually-exclusive-capable Cargo
features; egui must not be compiled when only webui is enabled.

## Core Interactions

### Query System
No changes. Reuses `ElementSource`, query submission via `UIContext`, and the
`AppRunner` evaluation loop unchanged. In browser mode, DOM events map to queries
exactly as egui click handlers do today.

### Store System
No changes. SSR mode may run against a server-side store; browser mode against a
wasm-compatible store. Store choice is out of scope here.

### Command System
Mirrors the egui backend's rendering commands. A `WebValueExtension` (analogous
to the egui `UIValueExtension::show`) renders a `Value` into DOM/HTML. UI-building
commands (the `lui` namespace) are framework-agnostic and already shared.

### Asset System
No changes. Reuses `AssetViewElement`, `AppRunner`, and the `AssetSnapshot`
update path; the web backend renders the same lifecycle states (progress → value
/ error) that egui renders.

### Value Types
Adds web rendering paths for existing base + `ExtValue` variants (text, image,
dataframe, metadata, etc.). The egui-only `ExtValue::UiCommand` and
`ExtValue::Widget` variants (and `show_in_egui`) become feature-gated behind
`egui`, so no egui types leak into a webui-only build. No new persistent value
variants are required for Phase 1.

### Web/API
SSR mode emits HTML strings that a server (future `liquers-axum` integration)
can embed. Axum wiring is out of scope for this feature; the SSR API is designed
to be embeddable but not delivered as an endpoint here.

### UI
Adds a web rendering entry point to the `UIElement` trait (a `show_in_web`-style
method + a `render_element_web` helper mirroring `render_element`), plus a
`ui/web` module holding the DOM/HTML renderers. Existing framework-agnostic
pieces (AppState, runner, message, resolve, shortcuts, handle) are reused as-is.

## Crate Placement

**liquers-lib** — all work lands here:
- New `liquers-lib/src/ui/web/` module (mirrors `src/egui/` role for the ui tree).
- Feature-gate the existing egui coupling in `src/ui/*` and `src/value/mod.rs`
  behind a new `egui` feature; add a `webui` feature enabling optional wasm deps
  (`web-sys`, `wasm-bindgen`, `js-sys`, `wasm-bindgen-futures`). `default` keeps
  egui on so existing builds/tests are unaffected.

No changes to liquers-core, liquers-macro, or liquers-store.

## Open Questions

1. **Rendering signature & DOM model.** Browser mode wants create-once +
   incremental `update()` (per UI_WEB_DESIGN_NOTES), while SSR wants a pure
   value→string render. Should one trait method cover both via a `WebSink`
   abstraction, or should browser (`web-sys`) and SSR (string) be two methods
   gated separately? → Resolve in Phase 2.
2. **egui feature-gating strategy.** How to remove `show_in_egui` and the
   egui-typed `ExtValue` variants from the trait/enum without breaking the many
   existing egui consumers and tests. → Resolve in Phase 2.
3. **SSR limited-support boundary.** Exactly which elements/values render
   server-side, and how interactive affordances (buttons, query console input)
   degrade to static markup or hydration hooks. → Resolve in Phase 2.
4. **Event wiring in browser mode.** How DOM events are delegated (via stable
   `ui-element-{handle}` IDs) into `AppMessage`/query submission through the
   existing `UIContext` channel. → Resolve in Phase 2.

## References

- `specs/UI_WEB_DESIGN_NOTES.md` — browser DOM rendering sketch (create-once + update, SSR hydration)
- `specs/UI_INTERFACE_FSD.md` — query-driven, platform-independent UI philosophy
- `specs/UI_RATATUI_DESIGN_NOTES.md`, `specs/UI_DIOXUS_DESIGN_NOTES.md` — other backend sketches validating the trait
- Reference backend: `liquers-lib/src/egui/` and `show_in_egui` in `liquers-lib/src/ui/element.rs`
- `liquers-lib/src/ui/` — AppState, runner, element, widgets, ui_context (framework-agnostic core)
