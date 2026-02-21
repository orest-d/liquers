# TECHNICAL-DEBT-1

Status: Draft

## Summary
Reduce core store-layer technical debt by:
1. removing remaining double-indirection patterns (`Arc<Box<...>>`) in `liquers-core`,
2. removing blocking store usage from core execution paths,
3. adding async-native memory and file store implementations.

## Problem
After the async-store cleanup, `liquers-core` still contains several `Arc<Box<...>>` fields and API signatures. This adds unnecessary allocation/indirection and complicates type signatures.

Known remaining cases:
1. `Arc<Box<dyn Store>>` (to be removed)
2. `Arc<Box<DefaultAssetManager<Self>>>`
3. `Option<Arc<Box<dyn AsyncRecipeProvider<Self>>>>`
4. matching return types in `Environment`/`EnvRef` methods
5. recipe-provider accessor in `DefaultAssetManager`
6. blocking `Store` trait and adapters still used in runtime flows
7. no first-class async memory/file store pair replacing blocking defaults

## Goals
1. Replace `Arc<Box<dyn Trait>>` with `Arc<dyn Trait>` where object-safe.
2. Replace `Arc<Box<ConcreteType>>` with `Arc<ConcreteType>`.
3. Remove dependency on blocking store APIs in core async execution paths.
4. Provide async-native `MemoryStore` and `FileStore` implementations.
5. Remove blocking `Store` from `Environment` trait/API surface and environment implementations.
6. Keep behavior unchanged where possible; this is primarily structural/infrastructure refactor.
7. Keep all core/lib tests green.

## Non-Goals
1. Broader context/session redesign.
2. Asset-manager architecture redesign.
3. API behavior changes beyond type cleanup.
4. Full removal of all legacy blocking-store code in one step, if compatibility shim is required during migration.

## Proposed Scope
1. Update `liquers-core/src/context.rs` field types and constructor wiring.
2. Update `Environment` trait signatures and `EnvRef` forwarding methods.
3. Update `liquers-core/src/assets.rs` recipe provider accessor type.
4. Introduce async-native `MemoryStore` and `FileStore` (or async wrappers with equivalent semantics).
5. Switch core runtime/store call-sites from blocking `Store` to async store interfaces.
6. Keep compatibility adapters only where needed during transition.
7. Propagate signature updates to implementing crates (`liquers-lib`, and others if needed).
8. Run test suites for at least:
   1. `liquers-core`
   2. `liquers-lib`

## Design
### 1. Async Store Direction
1. `AsyncMemoryStore` and `AsyncFileStore` become first-class implementations of `AsyncStore`.
2. Core runtime paths (`assets`, `interpreter`, recipe loading) should instantiate native async stores directly.
3. `AsyncStoreWrapper<T: Store>` is deprecated for runtime use and retained only as migration/testing compatibility shim.
4. `Store` (blocking) is treated as legacy compatibility API, not the default store path.
5. `Environment` should expose only async store accessors after migration; blocking store access is removed from environment-level API.

### 2. AsyncMemoryStore Design (`scc`-based)
1. Type:
   1. `AsyncMemoryStore { prefix: Key, data: scc::HashMap<Key, (Arc<[u8]>, Metadata)> }`
2. Rationale:
   1. `scc::HashMap` already exists in core and is optimized for concurrent access.
   2. Lock-free/striped behavior avoids global `RwLock` contention from current blocking `MemoryStore`.
3. Semantics:
   1. `set`: clone metadata, finalize metadata, store bytes as `Arc<[u8]>`.
   2. `get`: return owned `Vec<u8>` from `Arc<[u8]>` and cloned metadata.
   3. `set_metadata`: update metadata only if key exists (same behavior contract as current store unless explicitly changed).
   4. `listdir/listdir_keys/listdir_keys_deep`: compute from key scan of map entries; keep output parity with current `MemoryStore`.
   5. `contains/is_dir`: prefix-based checks mirroring existing semantics.
4. Optional optimization:
   1. maintain auxiliary `scc::HashMap<Key, ()>` directory index to avoid repeated full scans for large keyspaces.
   2. defer this optimization unless profiling shows need.

### 3. AsyncFileStore Design (async I/O + OS file locking)
1. Type:
   1. `AsyncFileStore { root: PathBuf, prefix: Key }`
2. Storage layout:
   1. data at `<root>/<key>`
   2. metadata at `<root>/<key>.__metadata__` (same as current `FileStore`)
3. Async I/O:
   1. use `tokio::fs::{create_dir_all, read, write, remove_file, remove_dir_all, read_dir}`
   2. write path uses atomic temp-write + rename:
      1. write data to `<path>.tmp.<pid>.<nonce>`
      2. `rename` to final path
      3. same for metadata file
4. Locking model (investigated design):
   1. Use per-key advisory OS lock file `<path>.__lock__`.
   2. Acquire exclusive lock for mutating operations (`set`, `set_metadata`, `remove`, `removedir`).
   3. Acquire shared lock for `get`/`get_metadata` if strict read-after-write consistency is required.
   4. Locking implemented through OS file locks (crate choice during implementation, e.g. `fs4`/`fd-lock`), using short critical sections.
5. Practical async note:
   1. OS file lock APIs are blocking at syscall boundary.
   2. Keep lock acquisition/release in short `spawn_blocking` sections if lock crate is sync-only.
   3. Keep all heavy file reads/writes in async `tokio::fs`.
6. Consistency guarantees:
   1. readers never observe partially written files (atomic rename).
   2. concurrent writers serialize per key via lock file.
   3. metadata/data update is best-effort two-file transaction (same as current model, but hardened).

### 4. Superseding `AsyncStoreWrapper`
1. New code must not create async stores from blocking `Store` via wrapper in production/runtime paths.
2. Replace current usages:
   1. `AsyncStoreWrapper(MemoryStore::new(...))` -> `AsyncMemoryStore::new(...)`
   2. `AsyncStoreWrapper(FileStore::new(...))` -> `AsyncFileStore::new(...)`
3. `store_builder` should produce native async store instances directly.
4. `AsyncStoreWrapper` remains temporarily for:
   1. incremental migration,
   2. tests covering legacy stores,
   3. third-party compatibility.
5. Add deprecation docs marker on wrapper and target removal milestone.

### 5. Migration Plan
1. Add `AsyncMemoryStore` and wire tests first.
2. Add `AsyncFileStore` with lock abstraction and atomic write path.
3. Update builders and environments to instantiate native async stores.
4. Migrate tests in `liquers-core` and `liquers-store` away from wrapper-based setup.
5. Mark blocking `MemoryStore`/`FileStore` as legacy and stop using them in async flows.
6. Remove wrapper usage from runtime crates (`liquers-core`, `liquers-lib`, `liquers-axum`) before final cleanup.

### 6. Test Strategy
1. AsyncMemoryStore parity tests:
   1. get/set/get_metadata/remove/removedir/listdir behavior parity vs current memory store.
   2. concurrent set/get stress test on same and different keys.
2. AsyncFileStore correctness tests:
   1. create/read/update/remove with metadata persistence.
   2. listdir/listdir_keys_deep with nested directories.
   3. concurrent writers on same key do not corrupt files.
   4. interrupted write (temp file left) does not surface partial value.
3. Migration tests:
   1. runtime integration tests no longer require `AsyncStoreWrapper`.
   2. `liquers-core` and `liquers-lib` test suites pass with native async stores.

## Acceptance Criteria
1. No remaining `Arc<Box<...>>` in `liquers-core` for the targeted cases.
2. Core async execution path does not require blocking `Store` methods.
3. Async memory and file stores exist and pass store behavior tests.
4. Runtime crates no longer depend on `AsyncStoreWrapper` for default store construction.
5. `cargo test -p liquers-core` passes.
6. `cargo test -p liquers-lib` passes.
7. No functional regressions in command/context/asset flows.

## Risks
1. Trait signature changes ripple into all environment implementations.
2. Type inference may need explicit coercions (`Arc<T>` to `Arc<dyn Trait>`).
3. Downstream crates may require coordinated updates.
4. Blocking-to-async migration may expose hidden sync assumptions in tests and utilities.

## Suggested Implementation Steps
1. Refactor core types and signatures.
2. Implement async memory/file stores with parity tests.
3. Migrate core call-sites away from blocking store APIs.
4. Fix compile errors in dependent crates.
5. Run tests and adjust failing assertions/await boundaries.
6. Final pass for consistency and formatting.
