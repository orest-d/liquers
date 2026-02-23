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
- Status: Open

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
