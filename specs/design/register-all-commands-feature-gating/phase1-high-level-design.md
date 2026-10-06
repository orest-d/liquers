# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The issue already identifies the only working mechanism: a `cfg` inside a
  `macro_rules!` body is evaluated in the caller's crate, so the gating must happen where
  `liquers-lib` defines the domain macros. One no-op twin per optional domain macro, defined
  under the negated `cfg`, makes the master macro compile everywhere.
- **Open questions:** None. Silently registering only the compiled-in domains is what the issue
  expects, and it matches `register_all_commands_fn`, which already uses `#[cfg]` per domain.

## Problem

`register_all_commands!` (`liquers-lib/src/commands.rs`) expands to `register_egui_commands!`,
`register_polars_commands!` and `register_records_commands!`, which are `#[macro_export]`ed from
modules gated on `egui`, `polars` and `records`. With a feature off, the expansion names a macro
that does not exist. `registry_export.rs` and `export_command_registry.rs` avoid the macro for
this reason and duplicate it by hand. `register_image_commands!` lives in an ungated module, but
its body references `image-support`-gated items. Phase 4 checks whether it compiles with that
feature off.

## Expected behaviour and acceptance

1. A crate depending on `liquers-lib` with `default-features = false` (and any single optional
   feature) can call `register_all_commands!(cr)` and compile.
2. The registered set equals the union of the enabled domains (same as the hand-mirrored loops).
3. `liquers-lib/tests/registry_export.rs` may use the macro instead of mirroring it (optional
   cleanup; the binary keeps its group selection).

## Scope

Macro definitions only. No command changes.

## Design Dependencies

None.

## Documentation assessment

- Guide: `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, if it mentions `register_all_commands!`'s
  feature limitation.
- Code docs: the master macro's doc comment and the two "deliberately not" comments.

## Consolidated Findings

- `#[macro_export]` places every exported macro at the crate root, so the twin must not be defined
  inside the gated module (it would vanish with it). Put the twins in `liquers-lib/src/commands.rs`
  next to the master macro, each under `#[cfg(not(feature = "…"))]`.
- `$crate::register_x_commands!` paths resolve to the crate root, so either twin is found.
- Validation is `scripts/check-build-matrix.sh` plus one test that calls the macro, run under
  `--no-default-features`.
