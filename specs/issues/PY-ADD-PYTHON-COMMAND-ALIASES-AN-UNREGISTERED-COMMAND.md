---
id: PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND
kind: issue
title: liquers-py's add_python_command registers an alias to a pycall command that does not exist
status: closed
priority: P2
complexity: M
area: [py]
design: liquers-py
created: 2026-10-17
github:
---

## Problem

`CommandMetadataRegistry.add_python_command` (`liquers-py/src/command_metadata.rs`) is compiled and
reachable from Python. It builds a `CommandMetadata` whose definition is
`CommandDefinition::Alias { command: CommandKey::new("", "", "pycall"), head_parameters: [module,
function, pass_state] }`. No `pycall` command is registered anywhere: it was defined only in
`liquers-py/src/commands.rs`, which was never compiled and has now been deleted.

## Impact

Every command declared through this method is metadata that can never execute: a query naming it
resolves to an alias whose target is missing. The method succeeds, so the failure surfaces only at
evaluation time, far from the call that caused it. No Python command can currently run at all, so
nothing works around it.

## Expected behaviour

Either of:

- Make `add_python_command` fail loudly (raise) until a Python command path exists, so a caller
  learns at declaration time that it is not supported.
- Replace it with the redesigned command path of the liquers-py implementation plan (to live under
  `specs/design/liquers-py/`): one Rust closure per Python command owning its callable, registered
  through `CommandDeclaration`, with no `pycall` alias.

## Discovery

Recorded as "Impact, point 2" of `PY-MODULES-NOT-DECLARED-IN-LIB`, which was closed on 2026-10-17
by deleting the orphaned `commands.rs`. Closing it would otherwise have left this live defect
untracked, so it is filed on its own.

## Resolution

`add_python_command` and the `pycall` alias are gone. A Python function is registered as an
async command whose closure owns the callable, through `@lq.command` / `@lq.first_command`
(`design/liquers-py/`, Phase 4 step 7, commit 2a987613; `test_COMMAND01`–`14`).
