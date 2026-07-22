# async-wasm-refactor Design Tracking

**Created:** 2026-07-22

**Status:** In Progress

## Phase Status

- [x] Phase 1: High-Level Design (drafted; awaiting user approval)
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Implementation Complete

## Notes

**Phase 1 finding (key):** Two separable blockers. (A) Runtime/`spawn` — raw `tokio::spawn`/`tokio::time` in `assets.rs`/`context.rs` panic on wasm; localized, non-architectural; fixable with a cfg-gated `rt` shim without touching `Send`. (B) `Send` bound — `Environment`/`AssetManager`/`CommandExecutor`/`AsyncStore`/`AsyncRecipeProvider` are hard `Send`-bound under `#[async_trait]`; architectural; only needed for browser-native I/O. A custom `Environment` alone can't dodge (A) because the two hot spawns live inside `DefaultAssetManager::with_capacity()` and `get_asset_manager()` returns the concrete manager. Recommend Tier 1 (shim) first, Tier 2 (conditional-Send) if browser I/O is a goal.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
