---
id: PY-MODULES-NOT-DECLARED-IN-LIB
kind: issue
title: Half of liquers-py's source files are not declared as modules and never compile
status: draft
priority: P2
complexity: M
area: [py]
design:
created: 2026-08-25
github:
---
## Problem

`liquers-py/src/lib.rs` declares nine modules. The crate contains **seventeen** source files. The
eight undeclared files are not part of the crate: they are never compiled, never type-checked, and
never reach Python.

| Declared in `lib.rs` | Present but undeclared |
|---|---|
| `command_metadata`, `dependencies`, `error`, `expiration`, `metadata`, `parse`, `plan`, `query`, `recipes` | `cache`, `commands`, `context`, `interpreter`, `state`, `store`, `value` |

**Partially addressed 2026-08-26** by `foreign-value-type-registration`: `value` and `context` are
now declared, and `value.rs` was repaired to compile (`try_into_query`'s return type,
`from_asset_info`'s signature and its `todo!()`, incompatible `match` arms, and four unimplemented
trait items). `cache`, `commands`, `interpreter`, `state` and `store` remain undeclared, and the
evidence below about `commands.rs` still stands. That work also made the crate testable: `pyo3`'s
`extension-module` moved behind a default feature, so `cargo test -p liquers-py --lib
--no-default-features --features async_store` links and runs.

`cargo check -p liquers-py --lib` succeeds with three deprecation warnings and no errors, because
the compiler never sees the orphaned files.

## Evidence that they are genuinely uncompiled

`liquers-py/src/commands.rs:162` reads `arg.parameters.0`, where `parameters` is declared
`pub(crate)` in `liquers-core/src/commands.rs:24`. A cross-crate read of a `pub(crate)` field does
not compile. That it does not fail is proof the file is not in the build.

## Impact

Two effects, the second worse than the first.

**1. The Python command-execution path does not exist.** `pycall`, `hello`, `greet`, `pyprint`, and
the `CommandArguments` / `CommandRegistry` `#[pyclass]` wrappers all live in `commands.rs`. None of
them is registered with PyO3, so none is reachable from Python.

**2. A compiled function builds an alias to a command that is never registered.**
`CommandMetadataRegistry::add_python_command` (`liquers-py/src/command_metadata.rs:402`) *is*
compiled, and it constructs

```rust
cmd.definition = CommandDefinition::Alias {
    command: CommandKey::new("", "", "pycall"),
    ...
};
```

`pycall` is defined only in the uncompiled `commands.rs`. So every command registered through the
Python API points at a target that cannot be resolved. This is a live, reachable API producing
metadata that can never execute — not merely dead code.

## Discovery

Found during the Phase 2 known-issue preflight of
`specs/design/variadic-arguments-declaration/`, while establishing whether `liquers-py` is a live
consumer of `ArgumentInfo.multiple`. It is, but only half: `add_python_command` sets
`last.multiple = true` (`command_metadata.rs:430`) and is compiled, while the `pycall` executor that
would consume the resulting `MultipleParameters` is not.

## Fix direction

Not simply "add the `mod` lines" — the orphaned files have drifted out of sync with `liquers-core`
and will not compile as they stand (the `pub(crate)` read above is one instance; there are likely
more, since these files have never been checked against any core refactor since they were orphaned).
Expect a port, not a re-declaration.

Sequence:

1. Declare one module at a time and fix what the compiler reports.
2. For `commands.rs` specifically, `arg.parameters.0` needs a public accessor.
   `CommandArguments::get_multiple` — added by
   `COMMAND-VARIADIC-ARGUMENTS-NOT-DECLARABLE` — covers the `MultipleParameters` branch at
   `commands.rs:163`; the surrounding loop needs a public way to iterate resolved parameters.
3. Decide deliberately whether each remaining file is worth reviving or should be deleted. A file
   that has not compiled in months may be better removed than ported.

Until then, `add_python_command` should arguably fail loudly rather than emit an unresolvable
alias.
