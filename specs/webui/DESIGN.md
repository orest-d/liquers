# webui Design Tracking

**Created:** 2026-07-20

**Status:** Implemented (SSR + wasm-compiles); browser runtime is a tracked follow-up.

## Phase Status

- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [~] Implementation: M1–M3 complete & tested; M4 partial (see below)

## Implementation Status (by milestone)

- **M1 — egui optional** ✅ `egui` Cargo feature; all egui coupling gated. Builds with/without egui.
- **M2 — shared plumbing** ✅ `UiAction` (custom string serde), `dispatch_action`, `AppMessage::ApplyToInput`
  + runner handling, shared `lui/submit` command, wasm-safe UI spawns via `spawn_ui_task`.
- **M3 — web backend + SSR** ✅ `render_web`/`show_in_web`, `value_to_html`, widgets, dataframe, menu
  rendering, `render_app_ssr`, `mount_web` browser driver. **SSR works; unit + SSR integration tests pass.**
- **M4 — browser example** ⚠️ **partial**:
  - ✅ **`polars` made optional** (wasm prerequisite — it pulled `object_store → hyper → tokio-net → mio`).
  - ✅ **workspace `resolver = "2"`** (stopped dev-dep `tokio net` leaking into the lib build).
  - ✅ **`liquers-lib` and the `examples-web/ui_spec_demo` crate compile to `wasm32-unknown-unknown`.**
  - ❌ **The example does not yet run in a browser**: the async evaluation engine calls `tokio::spawn`
    (in `liquers-core` `AssetManager`/`Context`), which panics on wasm (no runtime). Stock tokio compiles
    but panics; `tokio_with_wasm` does not compile because core's `#[async_trait] impl AssetManager`
    requires `Send`. See the follow-up below. Playwright e2e is therefore deferred.

## Follow-up (tracked)

**Make the async evaluation engine run on wasm.** Options: (A) make `liquers-core`'s async-trait
hierarchy `Send`-conditional (`#[async_trait(?Send)]` on wasm across `AssetManager`/`AsyncStore`/recipe
providers + the `+ Send` future bounds in `EnvRef`), then use `tokio_with_wasm`; or (B) route every core
`tokio::spawn`/`tokio::time` through an `Environment`-provided spawn/timer seam. Both are substantial
core changes; either unblocks the browser example + Playwright e2e. Documented in
`phase2-architecture.md` → "Browser Runtime & Workflow".

## Notes

- Design docs (Phases 1–4) reflect the approved design; the "Option A (keep tokio, verify by test)"
  runtime plan was tested and hit the `Send` wall — see the follow-up above and Phase 2.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
