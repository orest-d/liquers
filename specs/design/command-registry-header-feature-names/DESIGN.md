---
id: COMMAND-REGISTRY-HEADER-FEATURE-NAMES
kind: design
title: The command registry header names cargo features, not group names
status: complete
gh_pr: [149]
form: compact
readiness: ready
autofix: eligible
area: [lib/commands, build]
issues: [COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES]
created: 2026-10-17
---
# The command registry header names cargo features, not group names

> **Complete on 2026-10-18**: implemented and merged in #149.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-17 backlog compaction. Phases 1-4 were
implemented as written; see the completion note above.

## Phase 1: High-Level Design

### Purpose

The `# Origin:` line of `specs/command_registry.yaml` is how a reader reproduces the export. Its
`features=` list is built from `Group::name()` in `liquers-lib/src/bin/export_command_registry.rs`
`header`, so it prints group names where cargo features are meant.

### Problem Example

The committed header reads (line 4):

```
# Origin:   groups=core,lui,egui,image,polars,records,search,template,store_config  features=egui,image,polars,records,template
```

Two of the five "features" do not exist in `liquers-lib/Cargo.toml`: `image` (the feature is
`image-support`) and `template` (the feature is `templates`; the issue named only the first).
`cargo build -p liquers-lib --features image,template` fails. It should read
`features=egui,image-support,polars,records,templates`, which is also what the original design
specified (`specs/design/query-validation/phase2-architecture.md`, "features=egui,image-support,polars").

### Scope and Acceptance Criteria

- **AC-1** Feature names
  - WHEN the exporter writes the header with the default features
  - THEN `features=` lists `egui,image-support,polars,records,templates`, each declared in
    `liquers-lib/Cargo.toml` `[features]`, without duplicates
- **AC-2** Every group's feature exists
  - WHEN any `Group` names a cargo feature
  - THEN that feature is declared in `liquers-lib/Cargo.toml`
- **AC-3** Committed file corrected
  - WHEN `specs/command_registry.yaml` is regenerated
  - THEN only its `# Origin:` line and a CHANGELOG line change; `registry_export` still passes

Non-goals: listing `records-ipc` / `records-parquet` (they add no commands); the `groups=` list.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — tooling fix in one private `bin` of `liquers-lib`; no command,
  signature or `pub` item changes, and only a comment line of the generated registry changes.
- **Leading issue:** None
- **Explanation:** The intended output is stated by the issue and by the query-validation design;
  the change is a private method on a private enum.
- **Open questions:** None.

### Design Dependencies

None.

### Consolidated Findings

The `template` / `templates` mismatch is a second instance the issue did not list; the fix covers
both by construction (AC-2's test reads `Cargo.toml`).

## Phase 2: Architecture

### Solution

Add `fn feature(self) -> Option<&'static str>` to the private `Group` enum: `Core`, `Lui`,
`StoreConfig` → `None`; `Egui` → `"egui"`; `Image` → `"image-support"`; `Polars` → `"polars"`;
`Records` and `Search` → `"records"`; `Template` → `"templates"` (explicit arms, no `_`). Make
`is_compiled_in` use the same names (it already does, by `cfg!`). In `header`, build `features`
as `Group::all()` filtered by `is_compiled_in`, mapped through `feature()`, `flatten`ed and
deduplicated preserving order — which removes the `matches!(…Search…)` special case.

Rejected: renaming groups to match features (changes the `--group` CLI values).

### Changes

`liquers-lib/src/bin/export_command_registry.rs`: `Group::feature`, `header`, a `#[cfg(test)] mod
tests` at the end. `specs/command_registry.yaml`: regenerated header and one CHANGELOG line.

### Risks

None to behaviour; nothing parses the header (checked: no reader of `Origin:` in tree). Certainty:
high.

## Phase 3: Examples and Tests

### Examples

The Problem Example.

### Tests

In `export_command_registry.rs` `mod tests`:

- `group_features_are_declared_in_cargo_toml` — reads `include_str!("../../Cargo.toml")`, and
  asserts every `Group::feature()` value appears as a `[features]` key — AC-2
- `header_lists_cargo_features_once` (gated `#[cfg(all(feature = "egui", feature = "image-support",
  feature = "polars", feature = "records", feature = "templates"))]`) — the `features=` field is
  exactly `egui,image-support,polars,records,templates` — AC-1
- existing `liquers-lib/tests/registry_export.rs` `committed_registry_is_fresh` — AC-3

Run: `cargo test -p liquers-lib --features cli --bin export-command-registry`, then
`cargo test -p liquers-lib --test registry_export`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `export_command_registry.rs` `Group::feature`, `header` — `cargo test -p liquers-lib --features cli --bin export-command-registry`
- [x] 2. Regenerate `specs/command_registry.yaml` (CLAUDE.md command) and add a dated CHANGELOG line; check `git diff` shows only the header and that line — `cargo test -p liquers-lib --test registry_export`
- [x] 3. Close the issue with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

The two test commands above. Rollback: revert the commit.
