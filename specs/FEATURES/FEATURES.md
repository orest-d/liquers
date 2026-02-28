# Features Index

This folder contains larger feature specifications that should be detailed before implementation design.

## Feature List

1. `ASSETS-IMPROVEMENTS.md`
- Scope: asset persistence behavior, eviction safety, and upload limits.
- Status: Open
 - Implementation plan for Issue 4: `ASSETS-IMPROVEMENTS-ISSUE4-IMPLEMENTATION-PLAN.md`

2. `KEY-LEVEL-ACL.md`
- Scope: authorization for `set()` / `set_state()` on key patterns.
- Status: Open

3. `VALUE-DESCRIPTION.md`
- Scope: auto-generated, extensible value descriptions for all value types.
- Status: Open

4. `COMMAND-METADATA-ENHANCEMENTS.md`
- Scope: enum model (global/dynamic), specialization, and command input/output typing metadata.
- Status: Open

5. `EGUI-VALUE-RENDERING.md`
- Scope: complete egui rendering support for metadata-oriented value variants.
- Status: Closed

6. `EGUI-ASSET-MANAGER-INTEGRATION.md`
- Scope: stable adapter-based integration between egui widgets and asset manager.
- Status: Open

7. `POLARS-FEATURE-GAPS.md`
- Scope: separator/parquet capability gaps in Polars module.
- Status: Closed

8. `COMBINED-VALUE-DISCRIMINATION.md`
- Scope: discriminator-driven deserialization between base and extended value families.
- Status: Open

9. `TECHNICAL-DEBT-1.md`
- Scope: remove remaining `Arc<Box<...>>` indirection in `liquers-core` and align APIs to `Arc<...>` ownership.
- Status: Pending

10. `BENCHMARK-SUITE.md`
- Scope: define reproducible benchmark coverage for core runtime paths and technical-debt refactors.
- Status: Open

11. `IMAGE-SERIALIZATION-FEATURE-GAPS.md`
- Scope: unify image serialization/deserialization utilities and integrate `ExtValue::Image` with default value serialization.
- Status: Closed

12. `ASSETS-FIX1.md`
- Scope: resolve all `TODO`/`FIXME`/`todo!()` markers in `liquers-core/src/assets.rs` with prioritized implementation plan.
- Status: Draft

13. `ASSETS-FIX1-PHASE1-RUNTIME-BLOCKERS.md`
- Scope: remove runtime blockers in assets execution (`Dependencies` panic stubs, delegation deadlock risk, expiration monitor wiring).
- Status: Draft
- Implementation plan: `ASSETS-FIX1-PHASE1-IMPLEMENTATION-PLAN.md`

14. `ASSETS-FIX1-PHASE2-METADATA-LIFECYCLE.md`
- Scope: consolidate volatility/expiration metadata lifecycle and fast-track corruption handling.
- Status: Draft

15. `ASSETS-FIX1-PHASE3-REFACTOR-API-CLEANUP.md`
- Scope: deduplicate run paths, integrate metadata save throttling, and migrate apply APIs toward state-based input.
- Status: Draft

16. `ASSETS-FIX1-PHASE4-NICE-TO-HAVE.md`
- Scope: asset reference caching, quick-plan/apply fast-track enhancements, and explicit create semantics.
- Status: Draft
