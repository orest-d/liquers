# Phase 1: High-Level Design — running liquers-core under an executor the embedder chooses

## Purpose

`VISION.md` keeps the core "minimal … so minimal and wasm builds stay viable" and makes it an
embeddable library. This design decides what "not tied to tokio" should mean for `liquers-core` and
plans the route, so an embedder (a `smol` or `async-std` application, a GUI event loop, a
single-threaded host) can evaluate queries without starting a tokio runtime.

## Problem Example

```rust
// A native application that uses smol, not tokio.
let envref = SimpleEnvironment::<Value>::new().to_ref();   // DefaultAssetManager
smol::block_on(envref.get_asset_manager().get_asset(&parse_query("greet")?))?;
```

- **Today:** `DefaultAssetManager::new` calls `tokio::spawn` for the job queue and the expiration
  monitor → panic "there is no reactor running". With `ImmediateEnvironment` the same call works
  for `greet` (proved by `immediate_runs_without_tokio_runtime`, which passes at HEAD), but panics
  as soon as a command logs (`METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH`) or a running asset is
  cancelled (`tokio::time::timeout` in `AssetRef::cancel`).
- **Should (recommended goal):** the immediate manager works under any executor for every flow;
  the queued manager works under any executor that the embedder hands it a spawner and a timer
  for; `tokio::sync` may remain as a dependency.

## Scope and Acceptance Criteria

- **AC-1** The immediate path needs no runtime in any flow
  WHEN the immediate manager evaluates, persists, logs, reports progress and cancels under `futures::executor::block_on` on a native target
  THEN no call panics for want of a tokio runtime
- **AC-2** The queued manager runs on a supplied executor
  WHEN a `DefaultAssetManager` is built with a non-tokio spawner and timer
  THEN its job queue, expiration monitor, background persistence and cancellation timeout use them, and the manager test suite passes under that executor
- **AC-3** The tokio-backed default is unchanged
  WHEN an environment is built without naming an executor on a native target
  THEN it behaves exactly as at HEAD, including under `#[tokio::test]`
- **AC-4** The wasm build keeps its surface
  WHEN `liquers-core` is built for `wasm32-unknown-unknown`
  THEN it needs no tokio runtime, timer or macro feature (it needs only `sync`, as today, or nothing under goal B)

Non-goals: `no_std` / embassy (the core uses `std` throughout — `std::sync`, `scc`, `regex`,
`chrono` — so the issue's embedded angle is out of reach of this change); `AsyncFileStore`'s
`tokio::fs` (a store choice, native only, replaceable by an OpenDAL `fs` store); test code
(`#[tokio::test]` stays).

## Core Interactions

Assets (every spawn and timer site), context (`Context`'s service channel and locks), store
(`AsyncFileStore`, the pipeline's group locks), environment construction (where an executor is
supplied).

## Crate Placement

`liquers-core` only, plus `liquers-lib`'s `DefaultEnvironment` construction if goal A adds a
builder argument. No new crate.

## Documentation Intent

- Reference: `PROJECT_OVERVIEW.md` (asset manager paragraph), `api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
  ("Manager implementations"), once a goal is chosen.
- Guide: none; a short "choosing an executor" note in `api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md`.
- Other documents: `liquers-core/Cargo.toml` comments.
- Documents to update: the above; `async-wasm-refactor` is complete and not edited.

## Design Readiness

- **Readiness:** phase2-blocked
- **Automatic fixing:** not-eligible — `XL`; a new trait or a dependency swap across the asset
  runtime (rules 1, 3, 4).
- **Leading issue:** which goal (question 1).
- **Explanation:** the measurement (Phase 2) shows two separable goals with disjoint change sets.
  Goal A (executor-agnostic) is a bounded seam over ~9 spawn and 3 timer sites; goal B (tokio-free)
  replaces ~40 `tokio::sync` sites with no behavioural gain. Planning either without the choice
  would design the wrong half. The one unambiguous defect is split off and ready.
- **Open questions:**
  1. **Blocking - goal:** (A) *executor-agnostic*: the embedder chooses the executor; `tokio::sync`
     stays (it is documented by tokio as runtime-agnostic and works under any executor), and only
     spawning and timers go behind a seam; or (B) *tokio-free*: no `tokio` crate in the core's
     dependency tree, replacing `Mutex`, `RwLock`, `mpsc`, `watch`, `Notify`, `OnceCell` with
     `async-lock` / `async-channel` / `event-listener` / `async-once-cell` (the issue's text and the
     `async-wasm-refactor` follow-up). Example of the divergence: under A, `cargo tree -p
     liquers-core --target wasm32-unknown-unknown` still lists `tokio` (feature `sync`) and the
     `smol` program above works; under B, `tokio` disappears from the tree but the `smol` program
     still panics until A's seam also exists. Recommended: **A first**; B only if a dependency
     audit asks for it, as its own issue.
  2. **Open design - where the executor is supplied (goal A):** a trait object on the manager
     (`DefaultAssetManager::with_runtime(Arc<dyn AsyncRuntime>)`) and an `EnvironmentBuilder`
     option, defaulting to a tokio implementation on native. Recommended. Alternative: an
     associated type on `Environment` (compile-time choice; a breaking change to every
     environment).
  3. **Proposed resolution - wasm:** with a seam, a wasm runtime (`wasm_bindgen_futures::spawn_local`
     plus a timer) could run the queued manager in the browser, which is the deferred manager
     `WEB-CANCELLATION-INERT` asks for. Recorded as a dependency, not in scope.

## Design Dependencies

- `inline-metadata-save-without-runtime` (`METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH`) —
  **requires**: AC-1's main gap; ready and eligible for automatic fixing, so it goes first.
- `environment-orderly-shutdown` (`ENVIRONMENT-ORDERLY-SHUTDOWN`) — **overlaps**: shutdown has to
  stop the tasks this seam spawns; whichever lands second uses the other's handle.
- `WEB-CANCELLATION-INERT` (issue) — **required-by** (optional): a deferred wasm manager can be the
  queued manager on a browser runtime once the seam exists.
- `evaluate-path-consolidation`, `async-wasm-refactor` (complete, E4) — origin of the spawn/inline
  harness split; not extended.
- `core-benchmark-suite` — **overlaps**: a runtime seam is a hot-path indirection to measure.
