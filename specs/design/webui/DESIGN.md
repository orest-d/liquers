---
id: WEBUI
kind: design
title: Web-framework-independent rendering with SSR and wasm
status: complete
area: [lib/ui]
gh_pr: [10]
issues: []
created: 2026-03-02
superseded_by:
---
# webui Design Tracking

**Created:** 2026-07-20


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

## Follow-up — RESOLVED ✅ (async-wasm-refactor, 2026-07-23)

**Make the async evaluation engine run on wasm.** DONE. Implemented as the `async-wasm-refactor`
feature (see `specs/design/async-wasm-refactor/`): a spawn-free `ImmediateAssetManager` (Axis 1 / b1)
selected via a new `Environment::AssetManager` associated type, plus target-gated conditional
compilation relaxing the core async traits to non-`Send` on wasm (`MaybeSend`/`MaybeSync` markers +
`BoxFuture` alias + `#[async_trait(?Send)]`). `DefaultEnvironment` cfg-selects `ImmediateAssetManager`
on wasm, so **this example runs unchanged in the browser**. wasm tokio reduced to `["sync"]`. The
deferred M4 Playwright e2e (`tests/webui.spec.ts`) now **passes in headless Chromium** — 1 passed.

## Follow-up: rendering follows the model (`specs/design/webui-fixes/`, 2026-07-25)

The browser loop originally re-rendered when `AppRunner::needs_repaint()` reported async work in
flight. With the inline asset manager that is almost never true at the moment it is asked, so menu
actions produced no DOM update at all — the M4 e2e passed only because its text assertion was
satisfied by a menu label before any click. `AppState` now records what changed (`UIChange` →
`Invalidation`) and the renderer applies it, performing real DOM inserts and removals for widgets
that declare a `data-lq-children` container. See `specs/design/webui-fixes/` and ISSUES
(WEBUI-REPAINT-AFTER-SYNC-MUTATION, resolved).

The *interaction* half — Enter not submitting in the query console, the submitted query not
reaching the element, and menu accelerators being egui-only — is designed separately in
`specs/design/ui-events/`.

## Notes

- Design docs (Phases 1–4) reflect the approved design; the "Option A (keep tokio, verify by test)"
  runtime plan was tested and hit the `Send` wall — see the follow-up above and Phase 2.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
