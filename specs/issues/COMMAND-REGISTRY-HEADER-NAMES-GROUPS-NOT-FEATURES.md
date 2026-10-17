---
id: COMMAND-REGISTRY-HEADER-NAMES-GROUPS-NOT-FEATURES
kind: issue
title: The command_registry.yaml header lists group names under "features"
status: draft
priority: P3
complexity: S
area: [lib/commands, build]
design: command-registry-header-feature-names
created: 2026-10-11
github:
---

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
