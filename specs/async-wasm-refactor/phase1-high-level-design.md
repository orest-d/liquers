# Phase 1: High-Level Design — Async / Send-Conditional Core Refactor (Browser/WASM)

## Feature Name

Async `Send`-conditional refactor of `liquers-core` for browser (`wasm32-unknown-unknown`) execution.

## Purpose

Make `liquers-core` actually **run** inside a browser tab, not merely compile to wasm. Today the crate compiles to `wasm32-unknown-unknown` but panics at runtime (no tokio reactor) and cannot host any browser-native I/O because every core async trait is hard-bound to `Send`. This refactor removes both blockers so the query engine, asset manager and command execution work under the browser's single-threaded JS event loop.

## Research Findings (the substance of this task)

The investigation shows **two distinct, separable blockers**. Conflating them is the reason prior attempts stalled.

### Blocker A — Runtime / `spawn` (runtime panic, *localized*, NOT architectural)

`liquers-core` calls **raw `tokio::spawn`** at ~10 sites and `tokio::time::{sleep,timeout}` / `select!` for timers:

- `assets.rs:2480` `job_queue.run()` and `assets.rs:2484` `run_expiration_monitor` — **spawned inside `DefaultAssetManager::with_capacity()`** (i.e. at construction).
- `assets.rs:187,1148,1440,3820,3985,4621,4653` — per-asset `process_service_messages` background loops.
- `context.rs:605` and `context.rs:732` — `load_command_versions()` spawned from `init_with_envref()`.
- `assets.rs` expiration monitor uses `tokio::time::sleep` + `tokio::select!`.

These **compile** on wasm (the `rt` feature is retained; `Cargo.toml` already cfg-drops `fs`/`net` for wasm) but **panic at runtime** — `tokio::spawn` requires a running reactor, and the browser main thread never enters one. `tokio::time` likewise needs a running time driver that does not tick on wasm.

**This is the blocker that stops the example from running.** Its fix is localized: a cfg-gated spawn/timer shim (`tokio::spawn` on native, `wasm_bindgen_futures::spawn_local` + `gloo-timers` on wasm). It does **not** require touching `Send`, because the futures currently spawned are already `Send` and `spawn_local` happily accepts `Send` futures.

### Blocker B — pervasive `Send` bound (compile-time, *architectural*, needed only for browser I/O)

Every core async abstraction is hard-bound to `Send`:

- `context.rs:43` `trait Environment: Sized + Sync + Send + 'static`
- `assets.rs:2248` `trait AssetManager<E>: Send + Sync` under `#[async_trait]`
- `commands.rs:407` `trait CommandExecutor<E>: Send + Sync` under `#[async_trait]`
- `store.rs:267` `trait AsyncStore: Send + Sync` under `#[async_trait]`
- `recipes.rs:305` `trait AsyncRecipeProvider<E>: Send + Sync` under `#[async_trait]`
- `context.rs:66,111` `evaluate`/`apply_recipe` return `Pin<Box<dyn Future + Send + 'static>>` explicitly.

`#[async_trait]` (default) boxes every method future as `Box<dyn Future + Send>`. Consequence: **no `!Send` value (a JS handle, a `web-sys` IndexedDB/`fetch` object, an `Rc`) may be held across an `.await` anywhere in the core execution path.** This is exactly why swapping in `tokio_with_wasm` failed to compile in the prior session — its single-threaded spawn is `!Send`-friendly, but core's `Send`-boxed async-trait futures reject it. Relaxing this is a cross-cutting change to ~5 traits and all their impls (and ripples into `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py`).

### Answers to the specific questions asked

- **Architectural or "just Send constraints"?** *Both, and separable.* The ability to **run** is blocked only by Blocker A (localized, non-architectural). The ability to do **browser-native I/O** (IndexedDB/`fetch` store, JS-interop commands) is blocked by Blocker B (architectural). If the goal is "core computes in the browser over an in-memory / pre-loaded store", Blocker A alone is enough.
- **A special `Environment` variant with a different initiation sequence?** *Helps, but insufficient alone.* A `WasmEnvironment` can rewrite `init_with_envref` (relocating the `load_command_versions` spawn), but the two hot spawns live inside `DefaultAssetManager::with_capacity()`, and `Environment::get_asset_manager()` returns the **concrete** `DefaultAssetManager` — so a custom environment cannot substitute a different manager. It cannot dodge Blocker A's core.
- **A special `AssetManager` variant?** *More promising for Blocker A, but needs a prerequisite.* The `AssetManager<E>` trait already exists; the seam is that `Environment::get_asset_manager` bypasses it with a concrete type. Generalizing that return to `Arc<dyn AssetManager<Self>>` lets a `WasmAssetManager` (or cfg-gated spawns inside `DefaultAssetManager`) use `spawn_local`. It does **not** address Blocker B — even a wasm manager must still satisfy `Send + Sync` today.

## Candidate Solutions (to be detailed in Phase 2)

- **Tier 1 — Spawn/timer shim (small, unblocks running).** New `liquers-core/src/rt.rs` with `spawn`/`sleep`/`timeout` cfg-gated (`tokio` native, `wasm_bindgen_futures`+`gloo-timers` wasm). Replace raw `tokio::spawn`/`tokio::time` call sites. Keep all `Send` bounds. Result: core runs in the browser with an in-memory / SSR-style store.
- **Tier 2 — Conditional `Send` (larger, unblocks browser I/O).** Introduce a `MaybeSend` marker alias (`= Send` on native, empty on wasm), switch core async traits to `#[async_trait(?Send)]` under `cfg(wasm)`, and relax the explicit `+ Send` future bounds via cfg. Optionally make `AssetManager` pluggable (`Arc<dyn AssetManager>`). Result: IndexedDB/`fetch` stores and JS-interop commands become possible. Ripples through all downstream crates.
- **Rejected / weaker:** `WasmEnvironment` alone (cannot avoid manager-internal spawns); `tokio_with_wasm` drop-in alone (fails to compile against `Send`-boxed async-trait futures — it presupposes Tier 2).

**Recommendation:** land Tier 1 first (self-contained, immediately makes the browser example run), then decide whether Tier 2 is in scope based on whether browser-native stores are a goal.

## Core Interactions

### Query System
No query-language or Key-encoding change. Query parsing is already `Send`-free synchronous code that compiles and runs on wasm; the refactor only affects how the *evaluation* of a parsed query is scheduled.

### Store System
`AsyncStore` gains a conditional-`Send` bound (Tier 2). Native backends (`AsyncFileStore`, OpenDAL) are unaffected on native. Tier 2 is the precondition for a future browser-native store (IndexedDB / `fetch`) that holds `!Send` handles across awaits.

### Command System
No new commands. `CommandExecutor` gains the conditional-`Send` bound (Tier 2); all existing registered commands and the `register_command!` macro output continue to compile unchanged on native.

### Asset System
The center of gravity. `DefaultAssetManager`'s background spawns (job queue, expiration monitor, per-asset service loops) move onto the `rt` shim (Tier 1). `AssetManager` trait bound relaxes under cfg (Tier 2). Optionally the manager becomes pluggable via `Arc<dyn AssetManager<Self>>`.

### Value Types
No new `ExtValue` variants. Value types are `Send` on native regardless; on wasm the relaxed bound simply permits (future) `!Send` values.

### Web/API (if applicable)
`liquers-axum` implements `Environment`/trait impls that must adopt the conditional bounds (Tier 2) but keeps full `Send` on its native server target — no behavioral change server-side.

### UI (if applicable)
Directly unblocks the `liquers-lib` `ui::web` browser driver (`mount_web`) whose runtime currently panics; it is the consumer this refactor exists to serve.

## Crate Placement

Core changes in **`liquers-core`** (new `rt` module + trait-bound edits). Tier 2 forces conforming edits in every downstream crate that implements the affected traits (`liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py`). No new user-facing crate.

## High-level summary — parts of the code that change

| File | Blocker A (spawn/timer) | Blocker B (Send) |
|---|---|---|
| `liquers-core/src/rt.rs` (new) | spawn/sleep/timeout shim | `MaybeSend` alias |
| `liquers-core/src/assets.rs` | manager + per-asset spawns, expiration timer | `AssetManager` bound + `async_trait(?Send)` |
| `liquers-core/src/context.rs` | `init_with_envref` spawn | `Environment` bounds, `evaluate`/`apply_recipe` future bounds, `get_asset_manager` return type |
| `liquers-core/src/commands.rs` | — | `CommandExecutor` bound + `async_trait` |
| `liquers-core/src/store.rs` | — | `AsyncStore` bound + `async_trait` |
| `liquers-core/src/recipes.rs` | — | `AsyncRecipeProvider` bound + `async_trait` |
| `liquers-core/Cargo.toml` | wasm deps: `wasm-bindgen-futures`, `gloo-timers` | — |
| `liquers-store`, `liquers-lib`, `liquers-axum`, `liquers-py` | — | trait impls adopt conditional bounds (Tier 2 only) |

## Open Questions

1. **Scope:** ~~Tier 1 only or Tier 1 + Tier 2?~~ **RESOLVED (user, 2026-07-22): Tier 1 first, then decide.** Phase 2 designs only the cfg-gated `rt` spawn/timer shim, keeping all `Send` bounds and avoiding downstream ripple. Tier 2 (conditional-`Send`) is deferred and re-evaluated after Tier 1 lands and the browser example runs.
2. **Threading model:** commit to single-threaded wasm (enables dropping `Send`), or keep the door open for wasm threads / `wasm32-wasi` (would keep `Send`)? → Phase 2.
3. **Shim vs. crate:** hand-rolled cfg shim (`wasm-bindgen-futures` + `gloo-timers`) vs. `tokio_with_wasm`? Prior session found the latter presupposes Tier 2. → Phase 2.
4. **AssetManager pluggability:** generalize `Environment::get_asset_manager` to `Arc<dyn AssetManager<Self>>` now, or keep concrete `DefaultAssetManager` and cfg-gate its internal spawns? → Phase 2.
5. **Native regression risk:** confirm `?Send`/`MaybeSend` leaves native builds byte-for-byte `Send` (no object-safety or inference regressions in `liquers-py`/`liquers-axum`). → Phase 2 + Phase 3 tests.

## References

- `specs/webui/DESIGN.md`, `specs/ISSUES.md` — prior session's async-on-wasm follow-up notes.
- `liquers-core/Cargo.toml` — existing wasm cfg (drops `fs`/`net`, keeps `sync`/`rt`/`macros`/`time`).
- Spawn/timer sites: `assets.rs:2480,2484,187,1148,1440,3820,3985,4621,4653`; `context.rs:605,732`.
- Send-bound sites: `context.rs:43,66,111`; `assets.rs:2248`; `commands.rs:407`; `store.rs:267`; `recipes.rs:305`.
- `wasm-bindgen-futures::spawn_local`, `gloo-timers`, `async-trait(?Send)`.
