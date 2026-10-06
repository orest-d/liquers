# Phase 2: Solution and Architecture

## Changes (removal)

| File | Change |
|---|---|
| `liquers-core/src/command_metadata.rs` | Delete `cache` and its doc. Delete `cache: true` in both constructors. Delete `true_default` if it becomes unused (check `volatile`/others). Fix the serialized-form test that expects `"cache":true`. |
| `liquers-core/src/command_declaration.rs` | Remove `assert_eq!(m.cache, k.cache)` |
| `liquers-lib/src/egui/widgets.rs` | Flags line: `volatile={}, async={}` |
| `liquers-py/src/command_metadata.rs` | Remove the `cache` getter |
| `specs/command_registry.yaml` | Regenerate with `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`, add a CHANGELOG line |

Confirm that no `#[serde(deny_unknown_fields)]` applies to `CommandMetadata` (none was found in
`command_metadata.rs`), so stored registries and declarations that still carry `cache` keep
loading.

## Alternatives

- **Wire it** (meaning: deterministic but not worth keeping). It needs consumers in the asset
  manager (skip the in-memory map but still store?), a macro statement, and a definition distinct
  from `volatile`. Rejected for this S issue. File a feature if wanted.
- **Reserve it** (document as reserved, default out of export). Same version cost as removal,
  and the field stays.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` | Changes what the registry test compares | No |

## Relevant commands

All registered commands' metadata changes shape. No command behaviour changes.

## Documentation architecture

Registry CHANGELOG line. Reference mentions (if any) removed with History rows.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | Table above |
| Workflows | Every command's metadata; egui command browser; Python bindings |
| Existing tests | Serialized-form test in `command_metadata.rs`; `command_declaration.rs` equality; `registry_export` (after regeneration) |
| Compatibility | Rust struct-literal callers break (semver-major for `liquers-core`). The Python getter is removed. Old serialized data loads. |
| Data | One-time `metadata_version` change for every command, so stored computed assets are expired and recomputed once |
| Recovery | Revert + regenerate the registry |
| Certainty | High once decided |
