---
id: COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES
kind: issue
title: The command_registry.yaml header lists group names under "features"
status: closed
priority: P3
complexity: S
area: [lib/commands, build]
design: command-registry-header-feature-names
created: 2026-10-11
github:
---
## Resolution (2026-10-17)

Fixed by design `command-registry-header-feature-names`. The private `Group` enum of
`liquers-lib/src/bin/export_command_registry.rs` gained `feature()`, mapping each group to the
`liquers-lib` cargo feature that compiles it in (`Image` → `image-support`, `Template` →
`templates`, `Records` and `Search` → `records`, `Core` / `Lui` / `StoreConfig` → none). `header`
builds `features=` from it, deduplicated in order, replacing the `search` special case. The
committed `specs/command_registry.yaml` header now reads
`features=egui,image-support,polars,records,templates`; only that line and a CHANGELOG line
changed.

Evidence: `group_features_are_declared_in_cargo_toml` (every group's feature is a `[features]` key
of `liquers-lib/Cargo.toml`) and `header_lists_cargo_features_once` in the binary's test module;
`registry_export` still passes.

## Problem

`liquers-lib/src/bin/export_command_registry.rs::header` writes
`# Origin: groups=…  features=…`, where `features` is built from `Group::name()` of every compiled-in
optional group. A group name is not always its cargo feature: `Group::Image` prints `image`, but the
feature is `image-support`. The header of the committed `specs/command_registry.yaml` therefore
reads `features=egui,image,polars,records`, naming a feature that does not exist.

## Problem Example

`specs/command_registry.yaml` line 4 reads `features=egui,image,polars,records,template`; neither
`image` nor `template` is a feature of `liquers-lib` (they are `image-support` and `templates`), so
`cargo build -p liquers-lib --features image,template` fails. Wanted:
`features=egui,image-support,polars,records,templates`.

## Impact

Cosmetic, but the header is what a reader uses to reproduce the export; `--features image` fails.

## Expected behaviour

`Group` has a `feature()` method, and the header prints the cargo feature names (deduplicated:
`search` shares `records`, which `header` currently special-cases).

## Discovery

While adding the `search` group for `design/store-and-asset-search/` Phase 4 Step 13, whose own
group needed excluding from the list for the same reason.
