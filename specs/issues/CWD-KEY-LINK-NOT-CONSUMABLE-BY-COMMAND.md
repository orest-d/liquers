---
id: CWD-KEY-LINK-NOT-CONSUMABLE-BY-COMMAND
kind: issue
title: A `-R-key/` link cannot be consumed as a command argument
status: draft
priority: P1
complexity: S
area: [core/commands, core/query]
design: plan-cwd-freeze
created: 2026-08-15
github:
---
## Problem

`-R-key/<key>` plans to `Step::UseKeyValue` and evaluates to `Value::Key`. As a **link argument** to
a command, that value cannot be converted into anything a command can declare:

- `Value::Key` has no `try_into_string` arm. `ValueInterface::try_into_string`
  (`liquers-core/src/value.rs:297-307`) handles `None`, `Bool`, `I32`, `I64`, `F64`, `Text` and
  `Bytes`, and falls through to `Error::conversion_error` for everything else. So
  `fn cmd(state, dir: String = query "-R-key/.")` fails at argument conversion.
- `Key` is not a declarable argument type. `FromParameterValue` is implemented only through
  `impl_from_parameter_value2!` for `String`, the integer types, floats and `bool`
  (`liquers-core/src/commands.rs:215-235`), so `fn cmd(state, dir: Key = query "-R-key/.")` does not
  compile.

`Value::Key` is otherwise well supported — `try_into_key` (`value.rs:551`), `as_bytes`
(`value.rs:813`), and the `key`/`txt`/`text/plain` metadata arms all exist. The gap is specifically
the command-argument boundary.

## Impact

This blocks the mechanism `plan-cwd-freeze` depends on. That design makes `Context::get_cwd_key`
crate-private and replaces it with a `-R-key/.` link argument, on the grounds that the directory
should reach a command as explicit, overridable data rather than ambient context. A command cannot
currently receive it, so the replacement does not exist yet and the accessor cannot be narrowed.

More generally, any command wanting a key as an argument — not a key's *contents*, the key itself —
has no way to declare one, which affects link arguments pointing at `-R-key/`, `-R-dir/` listings,
and anything that passes a location rather than a payload.

## Expected behaviour

A command can declare a key-valued argument and receive it from a `-R-key/` link. Either:

1. **`FromParameterValue for Key` plus `TryFrom<Value> for Key`**, so `dir: Key` is declarable. This
   is the typed option and matches how `Key` is used everywhere else in the codebase.
2. **A `Value::Key` arm in `try_into_string`**, returning `k.encode()`, so `dir: String` works and
   the command parses it back. Cheaper, but it widens String conversion for every command that
   takes a `String` and is handed a key — previously an error, silently a success afterwards.

Option 1 is the better fit. Option 2 is worth considering only alongside it, and deliberately: note
that `try_into_string`'s fallthrough at `value.rs:306` is a `_ =>` default arm, which the project's
match convention prohibits, so touching that function invites a wider cleanup.

## Discovery

Found while implementing `specs/design/plan-cwd-freeze/` Phase 4 step 10, migrating
`liquers-core/tests/recipe_cwd_resolution.rs`'s `cwd` and `append_cwd` commands off
`Context::get_cwd_key`. Phase 2 recorded "no new value types — `-R-key/.` yields `Value::Key`, which
already exists"; existing as a value turned out not to imply being consumable as an argument.
