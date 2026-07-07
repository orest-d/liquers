# Phase 1: High-Level Design - liquers-wf

## Feature Name

liquers-wf — Liquers WASM Web Framework

## Purpose

Provide a WASM-compiled crate that exposes Liquers query/command/store functionality to the browser through a JavaScript API. It lets a web page define commands in JavaScript, execute Liquers queries, choose a browser-appropriate store (memory / localStorage / IndexedDB), and optionally evaluate the URL hash fragment and render the result into a designated HTML element. The crate is feature-gated so a minimal build (query engine + memory store) stays compact, while a full build adds persistent stores and the liquers-lib egui UI.

## Core Interactions

### Query System
Reuses `liquers-core` `Query`/`parse_query` unchanged. The URL hash (`#...`) is parsed as a query and evaluated; results are dispatched to a target element.

### Store System
Adds browser store backends implementing `AsyncStore`: `MemoryStore` (reuse existing), `LocalStorageStore`, and `IndexedDbStore`. Selected via JS config. On wasm32 these wrap non-`Send` web-sys handles to satisfy the `AsyncStore: Send + Sync` bound (single-threaded runtime).

### Command System
Introduces a `JsCommandExecutor` allowing commands to be registered from JavaScript (name, namespace, JS callback). It composes with the existing `CommandRegistry` so native Rust commands and JS commands coexist. No new query syntax.

### Asset System
Reuses the existing `AssetManager`/`AssetRef` lifecycle; the JS API surfaces asset evaluation, status, and results. Background work uses the existing `spawn_ui_task` (already `spawn_local` on wasm32).

### Value Types
No new `ExtValue` variants. Adds bidirectional `Value` ⇄ JS (`JsValue`) conversion helpers (JSON/bytes/string bridge).

### Web/API
This crate is the browser-side counterpart to `liquers-axum` (which is native/server). Public surface is a `#[wasm_bindgen]` JS API, not HTTP endpoints.

### UI
Full-feature build supports the `liquers-lib` UI (egui via eframe on a browser canvas) so `AssetViewElement`/`UISpecElement` render in the browser. Minimal build renders results as text/HTML into a target element without egui.

## Crate Placement

New workspace crate **liquers-wf**, depending on `liquers-core`, `liquers-store` (config reuse where applicable), and optionally `liquers-lib` (behind a `ui` feature). It sits at the end of the dependency flow, parallel to `liquers-axum`. Feature flags: `default = ["localstorage"]`; `indexeddb`, `ui`, `js-commands`, and a `minimal` profile enabling only memory store + query execution.

## Open Questions

1. `Send + Sync` bounds on `Environment`/`AsyncStore` vs. non-`Send` web-sys/JS handles — resolve via `send_wrapper::SendWrapper` (single-threaded wasm) or an alternate wrapper? (Phase 2)
2. IndexedDB is inherently async and callback-based — how to bridge to `AsyncStore`'s `async_trait` methods (promise→future via `wasm-bindgen-futures`)? (Phase 2)
3. Hash-execution model: eager on load + on `hashchange`, or opt-in via JS call? Default target element resolution (id/selector)? (Phase 2)
4. JS command re-entrancy: a JS command may itself call `evaluate` — how to avoid borrow/lifetime issues across the JS callback boundary? (Phase 2)
5. Should store config reuse `liquers-store` `StoreConfig` (serde) or a dedicated JS-friendly config object? (Phase 2)
6. egui-in-browser rendering surface (eframe web) and its bundle-size cost — is `ui` strictly opt-in and excluded from the minimal build? (Phase 2)

## References

- `liquers-lib/src/ui/mod.rs:53-69` — existing wasm `spawn_local` abstraction
- `liquers-core/src/context.rs:42` — `Environment` trait (`Send + Sync + 'static`)
- `liquers-core/src/store.rs:267` — `AsyncStore` trait; `AsyncMemoryStore` at :501
- `liquers-core/src/commands.rs:407` — `CommandExecutor` trait
- `liquers-store/src/config.rs` — `StoreRouterConfig`/`StoreConfig`
- `specs/PROJECT_OVERVIEW.md`, `specs/ASSETS.md`, `specs/UI_INTERFACE_PHASE1_FSD.md`
