---
id: COMMAND-MACROS-NEED-CALLER-IMPORTS
kind: issue
title: The exported register_*_commands! macros compile only with Context and State imported by the caller
status: draft
priority: P3
complexity: S
area: [lib/commands]
created: 2026-11-21
github:
---
## Problem

`register_records_commands!` (`liquers-lib/src/records/commands.rs`) expands to
`register_command!` calls whose generated wrappers name `Context` and `State` unqualified. The
macro imports the command functions (`use $crate::records::commands::*`) but not those types, so a
caller outside `liquers-lib` that has not imported `liquers_core::context::Context` and
`liquers_core::state::State` gets `cannot find type Context in this scope` from the expansion.
The caller must also define `type CommandEnvironment`, which the macro docs say, but the two
imports are not documented anywhere.

Seen while writing the module example of `records/commands.rs` (query-editor-ergonomics Step 11):
the doctest needs a hidden `use liquers_core::{context::Context, state::State};`.
`liquers-lib/tests/command_icons.rs` works around it the same way, with `PhantomData` lines to
keep the imports used. Other exported macros (`register_image_commands!`,
`register_polars_commands!`) may have the same gap; not checked.

## Expected

An exported registration macro compiles in a caller that has only `type CommandEnvironment`
defined: it either imports what its expansion names (`use liquers_core::context::Context;` …
inside the block) or `register_command!` emits fully qualified paths.
