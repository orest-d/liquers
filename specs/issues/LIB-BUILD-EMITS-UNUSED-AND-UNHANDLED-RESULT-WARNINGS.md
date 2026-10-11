---
id: LIB-BUILD-EMITS-UNUSED-AND-UNHANDLED-RESULT-WARNINGS
kind: issue
title: A default liquers-lib build emits about 50 warnings, some for discarded Results
status: draft
priority: P3
complexity: S
area: [lib/commands, build]
design:
created: 2026-10-11
github:
---

## Problem

`cargo run -p liquers-lib --features cli --bin export-command-registry` (default features) prints
about 50 warnings before any output. Most are unused imports: `register_command`,
`CommandRegistryAccess`, `CommandRegistry` and `ValueInterface` in every `liquers-lib/src/polars/*.rs`,
`liquers-lib/src/commands.rs`, `egui/commands.rs`, `image/commands.rs`, `ui/widgets/ui_spec_element.rs`.
Some are not cosmetic:

- `unused_must_use` on `context.info(…)` in `liquers-lib/src/commands.rs:66,72,78` and on
  `context.warning(…)` in `liquers-lib/src/ui/commands.rs:298` — a failed log write is silently
  dropped;
- `unused_assignments` on `df = sorted_df` in `liquers-lib/src/egui/dataframe.rs:39` — the sorted
  frame is never read, which looks like a lost sort;
- `unused variable: filter` in `liquers-lib/src/image/geometric.rs:81` — a parsed resize filter
  that is never applied.

## Impact

New warnings from a change are invisible among the existing ones, and the three non-cosmetic ones
may each hide a wrong result.

## Expected behaviour

A default build of `liquers-lib` is warning-free; the three suspicious sites are checked for the
behaviour they appear to lose, and fixed or explained.

## Discovery

While regenerating `specs/command_registry.yaml` for `design/store-and-asset-search/` Phase 4 Step
13. Not fixed there: none of the sites is in the search code.
