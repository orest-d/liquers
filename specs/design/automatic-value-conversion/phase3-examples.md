# Phase 3: Examples & Use-cases — automatic conversion of a command's state and link arguments

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A declared state is converted | `make_i32-21/double_i64` | AC-1, AC-4 |
| 2 | Example | A refused edge passes through | `make_text-21/double_i64` | AC-3 |
| 3 | Test | `liquers-core/tests/automatic_conversion.rs::state_widened_and_logged` | conversion, info entry | AC-1 |
| 4 | Test | `…::accepted_state_untouched` | no entry | AC-2 |
| 5 | Test | `…::refused_or_missing_edge_passes_through_with_warning` | Fallible, none, failing edge | AC-3 |
| 6 | Test | `…::converted_state_metadata` | identifier, data format | AC-4 |
| 7 | Test | `…::link_argument_converted` | `add_i64-~X~make_i32-21~E` | AC-5 |
| 8 | Test | `…::undeclared_command_unchanged` | same command undeclared | AC-6 |
| 9 | Test | `…::first_reachable_declared_type_wins` | order of `accepts` | AC-7 |
| 10 | Test | `…::declaration_format_command_converted` | `CommandDeclaration` path | AC-8 |

## Example 1: a declared state is converted

Setup (core `Value`, scalar edges of `value-conversion-registry`): an environment built with the
default conversion registry, a memory store, and

```rust
fn make_i32(n: i32) -> Result<Value, Error> { Ok(Value::from_i32(n)) }
fn double_i64(state: &State<Value>) -> Result<Value, Error> {
    match &*state.data {             // strict on purpose: `TryFrom<Value> for i64` also takes I32
        Value::I64(n) => Ok(Value::from_i64(2 * n)),
        other => Err(Error::conversion_error(other.identifier(), "I64")),
    }
}
register_command!(cr, fn make_i32(n: i32) -> result produces: "I32")?;
register_command!(cr, fn double_i64(state) -> result accepts: "I64" produces: "I64")?;
```

`make_i32-21/double_i64` evaluates to `I64(42)`; the asset log holds
`converted state I32 → I64 (Widening) for 'double_i64'`; the state `double_i64` saw has type
identifier `I64` and no data format. (Both queries validate with `liquers-validate --command`.)

## Example 2: a refused edge passes through

With `fn make_text(n: i32) -> Value::from_string(n.to_string())` declared `produces: "Text"`,
`make_text-21/double_i64` logs a warning `state: no automatic conversion Text → [I64] (Fallible);
passed unchanged to 'double_i64'`, and `double_i64` fails with its own conversion error, exactly as
it does today. Under the strict alternative of question 1 the failure would come from the
interpreter instead, before `double_i64` runs.

## Edge and Error Cases

- `accepts` lists the actual type: no lookup in the conversion registry (test 4).
- A `Structural` edge that fails on this value (an `Array` of numbers to `RecordView` in
  `liquers-lib`; in core a test edge registered with a failing function): warning, pass-through
  (test 5).
- Several declared types, only the second reachable: converted to the second (test 9).
- An alias step: the target's metadata is the one consulted (the step names the target).
- A variadic argument's elements are not converted (their links are resolved element-wise before
  `CommandArguments` is built; out of scope, noted for a follow-up if a command needs it).
- An input in error status never reaches the arm (`do_step_state` stops on it first).
- `Metadata::LegacyMetadata` input: value converted, metadata kept as is (no identifier field to
  update), logged as usual.

## Test Plan

- New `liquers-core/tests/automatic_conversion.rs`, environment as in
  `liquers-core/tests/async_hellow_world.rs` (`AsyncMemoryStore::new(&Key::new())`), commands of
  Example 1-2 plus `add_i64(state, n: Value (accepts: "I64"))`; each test evaluates a query and
  inspects the value and the asset log (tests 3-10).
- `cargo test -p liquers-core --test automatic_conversion`; then `cargo test -p liquers-core` and
  `cargo test -p liquers-lib --lib --tests` for regressions.
