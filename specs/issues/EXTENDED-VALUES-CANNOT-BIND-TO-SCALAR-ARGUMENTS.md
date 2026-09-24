---
id: EXTENDED-VALUES-CANNOT-BIND-TO-SCALAR-ARGUMENTS
kind: issue
title: An extended value can never bind to a scalar command argument, even one it can convert to
status: draft
priority: P2
complexity: S
area: [lib/value, core/commands]
design:
created: 2026-09-24
github:
---
# An extended value can never bind to a scalar command argument, even one it can convert to

## What is wrong

`CombinedValue` has two conversion paths to a scalar, and they disagree.

- **`ValueInterface::try_into_string`** delegates to the extension
  (`liquers-lib/src/value/extended.rs`, `CombinedValue::Extended(ext) => ext.try_into_string()`),
  and `ValueExtension` has a default-refusing `try_into_string` hook an extension can override.
- **`try_into_i32`, `try_into_i64`, `try_into_f64`, `try_into_bool`** refuse every extended value
  outright. `ValueExtension` has no hook for them, so no extension can opt in.
- **`TryFrom<CombinedValue<B, E>>` for `i32`, `i64`, `f64`, `bool` and `String`** all refuse
  `Extended` with a `conversion_error` — **including `String`**, although the `ValueInterface` path
  for `String` delegates.

The `TryFrom` path is the one that matters: a **resolved link argument** is converted with
`T::try_from(value)` (`liquers-core/src/commands.rs`, `CommandArguments::get`). So an extended value
reaching a command through a `links:` entry in a recipe cannot bind to an `i64`, `f64`, `bool` or
`String` parameter, whatever the extension implements.

## Why it matters

Any extended value with a natural scalar reading is locked out of argument binding. The case that
exposed it: a one-row, one-column record view (`record-streams` Phase 2) that should read as a
number — `-R/data/prices.csv/-/ns-rec/rec_id-42/cols-price` linked into a command's `price: f64`
argument. It is also the smaller half of `VALUE-CONVERSION-CAPABILITY`: that issue wants a
conversion registry, but even without one, a value that *knows* its scalar reading has no way to
offer it.

## Expected behaviour

`ValueExtension` gains default-refusing hooks for every scalar `ValueInterface` conversion
(`try_into_i32`, `try_into_i64`, `try_into_f64`, `try_into_bool`, plus the `_option` forms where
they exist); `CombinedValue`'s `ValueInterface` impl delegates to them; and the `TryFrom` impls
delegate to the same hooks rather than refusing. Default bodies keep every existing extension
unchanged. The two paths must then agree, which a test can assert per scalar type.

## Discovery

Found 2026-09-24 while analysing whether a record view with one row and one column can be read as a
scalar (`specs/design/record-streams/`, traits-and-views discussion).
