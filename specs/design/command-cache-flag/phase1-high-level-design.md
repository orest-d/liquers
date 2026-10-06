# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Decided (Maintainer decision, 2026-10-06): remove the field. The one-time consequence (every command's
  `metadata_version` changes, so stored computed assets are recomputed once after the upgrade) is
  acceptable at this stage.
- **Open questions:** None. The Python `cache` getter is removed with the field. Keeping it would
  invent a meaning.

## Problem

`CommandMetadata.cache: bool` (`liquers-core/src/command_metadata.rs`) is public, documented,
defaulted to `true`, and exported into `specs/command_registry.yaml` for every command. Nothing
reads it for a decision, and `register_command!` cannot set it. The only reads are a test equality
(`command_declaration.rs`), the egui command-info "Flags:" line (`liquers-lib/src/egui/widgets.rs`),
and the Python getter (`liquers-py/src/command_metadata.rs`). `volatile` is the wired mechanism.

## Expected behaviour and acceptance

1. `CommandMetadata` has no `cache` field. Old YAML/JSON containing `cache` still deserializes
   (serde ignores unknown fields; the struct has no `deny_unknown_fields`).
2. `specs/command_registry.yaml` is regenerated without `cache` lines, with a CHANGELOG line that
   notes the one-time `metadata_version` change.
3. The egui widget shows `volatile` and `async` only.
4. `registry_export` passes. The build matrix passes.

## Design Dependencies

- `argument-info-description` and `command-metadata-command-hints` — **overlap** (same struct
  family and registry). Do all three in one release and regenerate the registry once.
- `command-registry-impl-version-freshness` — **overlaps** (registry export test).

## Documentation assessment

- Remove `cache` mentions from `specs/reference/COMMAND_DECLARATION.md` /
  `REGISTER_COMMAND_FSD.md` if any (search first).
- Generated: `specs/command_registry.yaml`.

## Consolidated Findings

- `metadata_version` is a hash of the serialized metadata
  (`CommandMetadataRegistry::calculate_metadata_version`), so any change of serialized form changes
  it. Accepted.
- Removing a public field breaks Rust struct-literal construction. In-tree literals are the two
  constructors in `command_metadata.rs` and tests.
