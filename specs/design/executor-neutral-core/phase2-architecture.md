# Phase 2: Solution & Architecture — running liquers-core under an executor the embedder chooses

## Overview

**Measured at HEAD (commit 18732dd, 2026-10-20).** `grep tokio` over `liquers-core/src`, library
code only (each file cut at its `#[cfg(test)] mod`), excluding `#[tokio::test]` (470 attributes):

| Category | Sites | Where | Compiled on wasm? |
|---|---|---|---|
| `tokio::spawn` | 9 | `assets.rs`: `MetadataSaver::save_immediately`, `AssetRef::new_temporary`, `persist_with_status_tracking` (background save, mode-checked), `run_with_future` (service loop), `JobQueue` construction and `submit`, `DefaultAssetManager::new` (queue + expiration monitor), `RunClaim::drop` | no (`cfg(not(wasm32))`) |
| `tokio::time` | 3 | `MetadataSaver` debounce `sleep`, expiration monitor `sleep`, `AssetRef::cancel` 5 s `timeout` | no |
| `tokio::select!` | 2 | `run_with_future`, expiration monitor | no |
| `JoinHandle` / `JoinError` | 2 | `MetadataSaverState::in_flight`; `PsmJoinError` alias (`Infallible` on wasm) | no |
| `tokio::sync` | ~40 | `assets.rs` (`mpsc`, `watch`, `Mutex`, `Notify`, `RwLock`; `key_mutation_lock`), `context.rs` (service `UnboundedSender`, three `Mutex`es), `dependencies.rs` (`expiration_lock`), `evaluation_session.rs` (`OnceCell`), `store/pipeline.rs` (group locks) | **yes** |
| `tokio::fs` | ~35 | `store.rs` `AsyncFileStore` | no (type declared unavailable on wasm, `store_factory.rs`) |

**How wasm copes today:** `liquers-core/Cargo.toml` gives wasm `tokio = { features = ["sync"] }`
only; 172 `target_arch = "wasm32"` conditions (41 in `assets.rs`) compile the spawn, timer and fs
sites out; `ImmediateAssetManager` with `run_inline` (`futures::join!` / `futures::select!`) drives
evaluation in the caller's task; wasm persists inline. Native `ImmediateEnvironment` follows the
same inline path, guarded by `immediate_runs_without_tokio_runtime`,
`immediate_keyed_eval_without_tokio_runtime` and `immediate_construction_without_tokio_runtime`
(all three pass, run 2026-10-20). The `tokio_exec` feature is an empty compatibility feature.

**Finding:** the fork is by *target*, but the property that matters is *mode*. On native, three
sites run on the inline path regardless of mode: `MetadataSaver::save_immediately` (split off,
`inline-metadata-save-without-runtime`), `AssetRef::new_temporary` (deprecated callers only) and
`AssetRef::cancel`'s timeout.

**Recommended solution (goal A, pending question 1):** one seam for the 9 spawn and 3 timer sites.

```rust
// liquers-core/src/runtime.rs — sketch
pub trait AsyncRuntime: MaybeSend + MaybeSync + 'static {
    /// Detached task. Completion is observed through channels, not join handles.
    fn spawn(&self, task: BoxFuture<'static, ()>);
    fn sleep(&self, duration: std::time::Duration) -> BoxFuture<'static, ()>;
}
#[cfg(not(target_arch = "wasm32"))]
pub struct TokioRuntime;          // the default: tokio::spawn / tokio::time::sleep
```

- `JoinHandle` uses are replaced by a `futures::channel::oneshot` completion signal (the service
  loop's result; the metadata saver's "in flight" flag), so the seam needs no handle type; a
  panicking task surfaces as a cancelled oneshot where `JoinError` surfaces today.
- `tokio::select!` becomes `futures::select!` (as `run_with_future_inline` already does), and
  `tokio::time::timeout` becomes a select against `runtime.sleep`.
- The runtime lives on the manager (`DefaultAssetManager`, `JobQueue`) and is reached from
  `AssetRef` through `envref.get_asset_manager()`, as `eval_mode()` already is; an
  `AssetManager::runtime()` accessor is added with a default returning `None` (inline managers),
  which is how the inline path proves it never spawns.
- `AsyncFileStore` keeps `tokio::fs` (native only; it is a store implementation, not the runtime).

Rejected: (B) swapping `tokio::sync` alone — the core would still panic without a tokio runtime;
a cargo feature selecting the executor — feature unification would force one choice on the whole
workspace, the reason `maybe_send` is target-gated rather than featured
(`async-wasm-refactor` Phase 2); an associated type on `Environment` (breaks every environment and
binding).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH` | draft | P2 | AC-1's main gap | yes | no | designed, ready |
| `ENVIRONMENT-ORDERLY-SHUTDOWN` | draft | P3 | stopping spawned tasks | no | no | coordinate |
| `WEB-CANCELLATION-INERT` | accepted | P3 | a browser runtime could host the queued manager | no | no | note |
| `CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS` | draft | P3 | touches the same `cfg` sites | no | no | note |
| `CORE-NO-DEFAULT-FEATURES-BROKEN` | — | — | reviews `tokio_exec` | no | no | note |

## Interfaces

Sketch above; nothing committed while question 1 is open. `AsyncRuntime` is synchronous to call
and returns boxed futures, so it is object-safe and needs no `async_trait`.

## Integration Points

`liquers-core/src/assets.rs` (the 9 + 3 sites, `MetadataSaver`, `JobQueue`, `DefaultAssetManager`,
`AssetManager` trait), `environment_builder.rs` (a `with_runtime` option), new
`liquers-core/src/runtime.rs`; `liquers-lib/src/environment.rs` only if `DefaultEnvironment` exposes
the option. `liquers-py` and `liquers-axum` keep the tokio default.

## Error Handling

A task that panics is reported as today's `JoinError` path does: the asset ends `Error` with
`Error::general_error` naming the asset ("service loop ended unexpectedly"). No new `ErrorType`.

## Relevant Commands

None.

## Documentation Architecture

`api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` ("Manager implementations": the runtime seam and the
inline guarantee), `PROJECT_OVERVIEW.md` (asset manager paragraph), `api/DOC_04_…` (executor note).
Proposed `affects_docs`: those three.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `assets.rs` (heavily), `environment_builder.rs`, new `runtime.rs` |
| Crates and workflows affected | core; every native environment through the default |
| Existing tests likely to change | none under goal A with the tokio default; tests asserting `JoinError` wording, if any |
| New validation | the manager suites (`manager_parametric.rs`) run under a second executor (`futures::executor::ThreadPool` + `async-io` timer, dev-dependency) |
| Compatibility / data / concurrency / performance / security | one dynamic dispatch per spawn and sleep (cold paths); `JoinHandle::is_finished` semantics replaced by a flag — the metadata saver's coalescing must keep "at most one worker" |
| Recovery | the tokio default keeps every current caller on today's behaviour |
| Certainty and open questions | Measurement certain; the seam is feasible; blocked on question 1 (goal) |
