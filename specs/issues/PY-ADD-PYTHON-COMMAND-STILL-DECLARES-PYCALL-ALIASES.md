---
id: PY-ADD-PYTHON-COMMAND-STILL-DECLARES-PYCALL-ALIASES
kind: issue
title: CommandMetadataRegistry.add_python_command is still public and declares aliases of a pycall command that does not exist
status: draft
priority: P3
complexity: S
area: [py]
design:
created: 2026-10-27
github:
---
## Problem

`PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` is `closed` with the resolution
"`add_python_command` and the `pycall` alias are gone", but at `0b98fa20`
`liquers-py/src/command_metadata.rs` still has `CommandMetadataRegistry.add_python_command` in its
`#[pymethods]` block, reachable as `lq.CommandMetadataRegistry().add_python_command(...)`. It builds
a `CommandMetadata` whose definition is `CommandDefinition::Alias` of `CommandKey("", "", "pycall")`
with `[module, function, pass_state]` as head parameters. No `pycall` command is registered
anywhere (`grep -rn pycall` finds only this method).

## Impact

A caller who finds the method (it is listed by `help(lq.CommandMetadataRegistry)`) declares
metadata for a command that can never run; nothing fails until a plan or evaluation reaches the
missing alias target. Python commands are registered with `@env.command` / `@lq.command`, so the
method has no legitimate use. Low impact: `CommandMetadataRegistry` is a metadata-only object used
with `build_plan`. The closed issue's resolution note is also untrue, which misleads a reader of
the record.

## Expected behaviour

One of (not chosen here; removing or changing a binding method is an interface change, so it is
not eligible for an automatic fix):

- remove `add_python_command` from the Python API, as the earlier resolution intended;
- make it raise `NotSupportedError` with a pointer to `@env.command`;
- keep it, document it as metadata-only, and correct the earlier issue's resolution note.

Until then its docstring (design `py-docstrings`) says that a command declared this way cannot
run.

## Discovery

Found on 2026-10-27 while writing the docstrings of `command_metadata.rs` for
`PY-PUBLIC-NAMES-HAVE-NO-DOCSTRINGS` (design `py-docstrings`, step 3), by reading the method and
searching the tree for `pycall`.
