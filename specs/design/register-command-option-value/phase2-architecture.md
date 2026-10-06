# Phase 2: Solution and Architecture

## Macro

`liquers-macro/src/registration.rs`, where the argument type is parsed (near
`argument_type_expression` / `is_option_of`):

```rust
const SUPPORTED_OPTION_INNER: &[&str] = &[
    "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize", "f32", "f64",
    // + "bool", "String" if question 2 is accepted
];
```

During parsing of `CommandParameter` (not codegen, so the span points at the user's type): if
`is_option_of(ty) == (true, Some(inner))` and `inner` is not in the list, return
`syn::Error::new_spanned(ty, format!("register_command!: argument '{name}' has type Option<{inner}>, \
which cannot be bound; supported Option types are Option<{list}>. For an optional value, use \
'{name}: String = \"\"' and parse it in the command"))`.

## Core (only if question 2 accepted)

`liquers-core/src/commands.rs`: add `impl_from_parameter_value2!(Option<String>, ...)` and
`Option<bool>` following the explicit `Option<i64>` pattern at the end of the impl list (null →
`None`, `as_str`/`as_bool` → `Some`, other → conversion error). Add `TryFrom<E::Value>` coverage
as the macro requires. Inspect how `impl_from_parameter_value2!` generates the `TryFrom` side for
`Option<i64>`.

## Alternatives

Full `Option<Value>` support (rejected for now, see Phase 1).

## Known-issue preflight

None.

## Relevant commands

None changed. `ns-rec` keeps `String = ""`.

## Documentation architecture

FSD table of argument types (supported `Option<…>`), guide sentence, History rows, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-macro/src/registration.rs`; maybe `liquers-core/src/commands.rs`; docs |
| Existing tests | Every in-tree registration must still expand. Run the whole lib test build. |
| New validation | trybuild-style compile-fail test if the macro crate has one, otherwise a unit test of the parser function on a parsed `syn::Type` |
| Compatibility | Code that failed with `E0277` now fails earlier with a clear message |
| Recovery | Revert |
| Certainty | High for rejection |
