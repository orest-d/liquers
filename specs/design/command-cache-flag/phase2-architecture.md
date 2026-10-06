# Phase 2: Solution and Architecture

| File | Change |
|---|---|
| `liquers-core/src/command_metadata.rs` | Delete `cache` and its doc; delete `cache: true` in both constructors (`new`, `from_key`); delete `true_default` only if unused afterwards; fix the serialized-form test expecting `"cache":true` |
| `liquers-core/src/command_declaration.rs` | Remove `assert_eq!(m.cache, k.cache)` |
| `liquers-lib/src/egui/widgets.rs` | Flags line: `volatile={}, async={}` |
| `liquers-py/src/command_metadata.rs` | Remove the `cache` getter |
| `specs/command_registry.yaml` | Regenerate with `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`; add a CHANGELOG line |

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | Same export test | No |

## Risk table

| Area | Assessment |
|---|---|
| Compatibility | Rust struct literals; the Python getter. Old serialized data loads. |
| Data | One-time `metadata_version` change, so recomputation once (accepted) |
| Recovery | Revert + regenerate |
| Certainty | High |
