# Phase 4: Implementation Plan

## Progress

- [x] 1. Core conversions
- [x] 2. `BooleanOption`
- [x] 3. Macro mapping and rejection
- [x] 4. Lib conversions and matches
- [x] 5. Python mirror
- [x] 6. Tests
- [x] 7. Documents and records

## Steps

1. `liquers-core/src/value.rs` `TryFrom<Value> for Option<T>` (and missing scalar widths);
   `commands.rs` `impl_from_parameter_value2_opt!(bool, …)`, remove the dead comment block. Proof:
   `cargo check -p liquers-core`.
2. `command_metadata.rs` `ArgumentType::BooleanOption` (`bool_opt`), `is_option`; `plan.rs`
   `from_string` arm. Proof: `bool_opt_serializes`, `cargo test -p liquers-core --lib`.
3. `liquers-macro/src/registration.rs`: `FloatOpt` → `FloatOption`, full option mapping,
   `SUPPORTED_OPTION_INNER` and the parse-time error. Proof: `cargo test -p liquers-macro`.
4. `liquers-lib`: `Option<T>` impls for `SimpleValue`/`CombinedValue`; the two `ArgumentType`
   display matches. Proof: `cargo test -p liquers-lib --lib --tests`.
5. `liquers-py/src/command_metadata.rs` mirrored variant. Proof: `cargo check -p liquers-py`.
6. `liquers-core/tests/register_command_option.rs` (Phase 3). Proof:
   `cargo test -p liquers-core --test register_command_option`; then
   `cargo test -p liquers-lib --test registry_export` (registry unchanged) and
   `bash scripts/check-build-matrix.sh`.
7. `REGISTER_COMMAND_FSD.md`, `COMMAND_REGISTRATION_GUIDE.md`, `CLAUDE.md` DSL lines (History,
   `reviewed:`); close the issue noting that `Option<String>`/`Option<Value>` are deliberately
   unsupported; `python3 scripts/docs_index.py --check`. Diff review.

Rollback: each step is a revert; nothing persisted depends on the new variant until a command uses
it.

## Outcome (2026-10-11)

Steps 1–7 done. The Phase 3 tests pass (`liquers-core/tests/register_command_option.rs`, plus
`option_widths_convert`; `bool_opt_serializes`; the macro's `option_unsupported_inner_is_rejected`,
`option_f64_maps_to_float_option`, `option_widths_and_bool_map_to_option_types` and
`injected_option_is_not_checked`). The three review findings in Phase 2 are resolved:

1. `TryFrom<Value> for bool` accepts `Value::Bool` (`option_from_link_none_and_value` links to a
   real boolean).
2. An optional argument with no default is not `ArgumentMissing`:
   `ParameterValue::from_arginfo` returns a null default for an option `ArgumentType`, so bare
   `opt_i` and `opt_b` give `None`. This is in the plan rather than the macro, so a manually
   registered option argument gets it too.
3. `SimpleValue` and `CombinedValue` gained `i8`, `i16`, `isize`, `u16`, `u64` and `usize`
   (through `i64`, refusing out-of-range values), and every `Option<T>` impl.

Differences from Phase 2:

- **Plain integer widths map to `Integer` too.** `i8`, `i16`, `u8`, `u16` and `usize` used to fall
  through to `Any`, although `REGISTER_COMMAND_FSD.md` already listed them as `Integer`; a query
  parameter then arrived as a string and failed to convert. `to_jpeg`'s `quality: u8` is the one
  registered command affected: it is now `int` in `specs/command_registry.yaml`.
- **`injected` arguments are not checked** for a supported `Option<T>`: they come from the context,
  not from a `Value`.
- `ArgumentType::is_option` lists every variant instead of a `_` arm.
