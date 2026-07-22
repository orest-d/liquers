# async-wasm-refactor Design Tracking

**Created:** 2026-07-22

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (drafted; awaiting user approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Implementation progress (execution)

- **M-A DONE (green, committed):** Step 1 `maybe_send` module; Step 2 Axis-2 trait relaxation (native no-op) across value/commands/store/recipes/context/assets/interpreter/plan. `cargo check -p liquers-core` green.
- **M-B spine DONE (green, committed):**
  - Step 4/8: `EvalMode`; shared `load_command_versions(dm,cmr)` helper; `AssetManager` trait extended (primitives incl. `remove_expired_from_maps` [B1] + shared default methods [Q1]); `DefaultAssetManager` conformed (`eval_mode=Queued`, `lookup_key_asset` via `assets.read_sync`, `start()`→helper, delegating primitives). `context.rs` init calls `start()`.
  - Step 5: `Environment::AssetManager` associated type; `get_asset_manager → Arc<Self::AssetManager>`; `Arc<Box<..>>` dropped; `SimpleEnvironment*` conformed.
- **M-B REMAINING (next focused push):**
  - **Step 7 `ImmediateAssetManager`** (new `assets_immediate.rs`) — mirror these `DefaultAssetManager` methods (assets.rs line refs) replacing `scc`→`std::sync::Mutex<HashMap>` and `job_queue.submit`→`run_inline`:
    - eval: `get_asset` (3023), `get` (3316), `get_query_asset` (2995)/`get_nonvolatile_query_asset` (2955)/`get_volatile_query_asset` (2974), `get_resource_asset` (2942), `apply` (3285), `apply_immediately` (3300).
    - store-delegating (map-free — candidates to hoist into trait *default* methods via a new `get_envref` primitive to avoid duplication): `recipe_opt` (3359), `is_volatile` (3370), `contains` (3739), `keys` (3749), `listdir` (3759), `listdir_keys` (3775), `listdir_keys_deep` (3784).
    - map-touching: `remove` (3387), `set_binary` (3427), `set_state` (3574), `get_asset_info` (3255), `makedir` (3814).
    - primitives: `eval_mode=Inline`, `lookup_key_asset` (Mutex get), `create_temporary_asset` (no-spawn temp), `start` (OnceCell + helper), `set_envref`, `dependency_manager`, `track_expiration` (no-op), `remove_expired_from_maps` (Mutex remove).
  - **Step 6/6b** — `run_inline`/`run_immediately_inline` (`futures::join!`+`select!` w/ fuse/pin [A1]); refactor `finish_run_with_result` to `Result<(),Error>`; cfg-gate Queued spawn/timer carriers: `run`/`run_with_future`/`run_immediately`/`new_temporary` (spawns), `MetadataSaver::save_immediately` Queued arm, `cancel` Queued arm, **`persist_with_status_tracking` spawn @1147 [B2]** (inline→synchronous `else` branch).
  - **Step 9** wasm `Cargo.toml` tokio `["sync"]`; **Step 3** cfg-out sync `Store`/`Cache`/`SimpleEnvironment*`; **macro** registration.rs:1118 emit `BoxFuture` (+ fixtures 1890/2358).
  - Also: test-manager impls at assets.rs ~4174/4199 (in `#[cfg(test)]`) need the new required trait methods before `cargo test` passes (M-D).
  - Checkpoint: native `cargo test -p liquers-core` + `cargo check --target wasm32-unknown-unknown -p liquers-core`.
- **M-C/M-D/M-E/M-F:** downstream conformance (lib/py/axum, `DefaultEnvironment` cfg-select + `init_with_envref` split + A2 `Instant::now()` audit), tests (parametric harness), Playwright e2e, docs.

## Notes

**Phase 1 finding (key):** Two separable blockers. (A) Runtime/`spawn` — raw `tokio::spawn`/`tokio::time` in `assets.rs`/`context.rs` panic on wasm; localized, non-architectural; fixable with a cfg-gated `rt` shim without touching `Send`. (B) `Send` bound — `Environment`/`AssetManager`/`CommandExecutor`/`AsyncStore`/`AsyncRecipeProvider` are hard `Send`-bound under `#[async_trait]`; architectural; only needed for browser-native I/O. A custom `Environment` alone can't dodge (A) because the two hot spawns live inside `DefaultAssetManager::with_capacity()` and `get_asset_manager()` returns the concrete manager. Recommend Tier 1 (shim) first, Tier 2 (conditional-Send) if browser I/O is a goal.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
