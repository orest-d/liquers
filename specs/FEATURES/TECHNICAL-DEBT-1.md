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
1. `Arc<Box<dyn Store>>`
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
5. Keep behavior unchanged where possible; this is primarily structural/infrastructure refactor.
6. Keep all core/lib tests green.

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

## Acceptance Criteria
1. No remaining `Arc<Box<...>>` in `liquers-core` for the targeted cases.
2. Core async execution path does not require blocking `Store` methods.
3. Async memory and file stores exist and pass store behavior tests.
4. `cargo test -p liquers-core` passes.
5. `cargo test -p liquers-lib` passes.
6. No functional regressions in command/context/asset flows.

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
