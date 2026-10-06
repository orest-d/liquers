# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — remove, wire, or reserve `cache`.** Someone has to
  say whether "cacheable" and "volatile" were ever meant to be independent axes.
- **Explanation:** All three options are feasible. Phases 3–4 specify the recommended removal,
  which has one non-obvious cost: changing the serialized form changes every command's
  `metadata_version`, which expires every dependent once on the next start.
- **Open questions:**
  1. **Proposed resolution — remove the field.** `volatile` is fully wired and covers the
     prototype's `cache=False` use. Nothing reads `cache`, no macro statement sets it, and the
     registry advertises it as a capability.
  2. **Open design question — accept a one-time version change.** Removing the key from the
     serialized metadata changes `metadata_version` for all ~108 registered commands. On upgrade,
     every stored computed asset whose recorded dependency includes a command version is expired
     once and recomputed. Alternatives: (a) accept (recommended for a pre-1.0 codebase; note it in
     the release notes); (b) exclude `cache` from the version hash only, which needs a
     version-hash exception mechanism that does not exist; (c) keep the field as reserved with
     `#[serde(skip_serializing_if = "is_true")]`. That changes the JSON too, so it gives no
     relief.
  3. **Proposed resolution — Python:** remove the `cache` getter from `liquers-py` (a breaking
     binding change, noted in the issue resolution). Keeping a getter that returns `!volatile`
     would invent a meaning.

## Problem

`CommandMetadata.cache: bool` (`liquers-core/src/command_metadata.rs`) is public, documented,
defaulted to `true` and exported into `specs/command_registry.yaml` for every command. No caching
decision reads it, and `register_command!` cannot set it. The only reads are a test equality
(`command_declaration.rs`), the egui command-info widget's "Flags:" line
(`liquers-lib/src/egui/widgets.rs`), and the Python getter.

## Expected behaviour and acceptance (removal)

1. `CommandMetadata` has no `cache` field. Old YAML/JSON with `cache: true|false` still
   deserializes (serde ignores unknown fields; `deny_unknown_fields` is not used on this struct,
   which Phase 4 re-checks).
2. `specs/command_registry.yaml` is regenerated without `cache` lines, with a CHANGELOG line.
3. The egui widget shows `volatile`, `async` only.
4. `registry_export` passes.

## Scope

Removal only. Wiring a "do not keep in memory" meaning would be a new feature with its own issue.

## Design Dependencies

- `argument-info-description` and `command-metadata-command-hints` — **overlap** (same struct
  family and registry). If they ship close together, regenerate the registry once.
- `command-registry-impl-version-freshness` — **overlaps** (registry export semantics).

## Documentation assessment

- Reference: `specs/reference/COMMAND_DECLARATION.md` / `REGISTER_COMMAND_FSD.md`: remove any
  `cache` mention (search first).
- Generated: `specs/command_registry.yaml` (regenerate, never hand-edit).
- Python: `liquers-py` docs, if they list getters.

## Consolidated Findings

- `metadata_version` is computed from the serialized metadata. Any change in serialized form,
  removal or default-skip, changes it. This is the real cost, and it is a decision, not a detail.
- Removing a public field is semver-breaking for Rust callers that construct `CommandMetadata`
  with struct literals. In-tree literals are in `command_metadata.rs` (two `new` paths) and tests.
