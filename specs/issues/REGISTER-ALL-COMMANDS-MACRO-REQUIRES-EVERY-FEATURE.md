---
id: REGISTER-ALL-COMMANDS-MACRO-REQUIRES-EVERY-FEATURE
kind: issue
title: register_all_commands! does not compile when an optional command feature is off
status: draft
priority: P3
complexity: S
area: [lib/commands]
design: 
created: 2026-09-25
github:
---
## Problem

`register_all_commands!` (`liquers-lib/src/commands.rs:323–332`) expands unconditionally to
`register_egui_commands!`, `register_image_commands!` and `register_polars_commands!`. Those macros
are `#[macro_export]`ed from modules gated on their features (`#[cfg(feature = "polars")] pub mod
polars;` in `liquers-lib/src/lib.rs`), so with any of those features off the expansion names a
macro that does not exist and the caller fails to compile. The limitation is known —
`liquers-lib/tests/registry_export.rs:40` and `src/bin/export_command_registry.rs:215` both avoid
the macro for this reason — but nothing records it as a defect.

A `#[cfg(feature = …)]` written *inside* the macro body would not fix it: it is evaluated against
the calling crate's features, not `liquers-lib`'s.

## Impact

Low. Only the egui examples call it, and they build with default features. Every other caller
mirrors the macro by hand, which is the duplication the macro was meant to remove. The
`record-streams` design adds a fourth optional domain (`register_records_commands!`), making the
macro one feature narrower again.

## Expected behaviour

`register_all_commands!` compiles under any feature combination and registers the domains that
are compiled in — for instance, each domain macro defined in both `cfg` arms, the disabled arm
expanding to `Ok(())`.

## Discovery

Found during the record-streams Phase 4 final review (2026-09-25) while specifying how
`register_records_commands!` joins the command set.
