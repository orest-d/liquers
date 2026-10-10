# Phase 4: Implementation Plan

## Progress

- [ ] 1. Core conversions
- [ ] 2. `BooleanOption`
- [ ] 3. Macro mapping and rejection
- [ ] 4. Lib conversions and matches
- [ ] 5. Python mirror
- [ ] 6. Tests
- [ ] 7. Documents and records

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
