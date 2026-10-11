# Phase 2: Solution and Architecture

## Core: the missing conversions

`liquers-core/src/value.rs`: `impl TryFrom<Value> for Option<T>` for `T` in `i8 i16 i32 i64 isize
u8 u16 u32 u64 usize f32 f64 bool` (a small `macro_rules!`): a none value (`ValueInterface::is_none`)
→ `Ok(None)`, otherwise `T::try_from(value).map(Some)`. Add the missing scalar `TryFrom<Value>` impls
the macro needs (`i8`, `i16`, `u8`, `u16` if absent). Concrete impls, not a blanket
`impl<T> TryFrom<Value> for Option<T>`, which risks overlap with core's `TryFrom<U> for T where U:
Into<T>`.

`liquers-core/src/commands.rs`: `impl_from_parameter_value2_opt!(bool, |p| p.as_bool())`; delete the
commented-out `Option<i64>`/`Option<f64>` block.

`liquers-lib/src/value/`: the same `Option<T>` impls for `SimpleValue` (`simple.rs`) and
`CombinedValue<B, E>` (`extended.rs`), beside the existing scalar ones.

## Core: `BooleanOption`

`liquers-core/src/command_metadata.rs` `ArgumentType`: add

```rust
#[serde(rename = "bool_opt")]
BooleanOption,
```

and the same in `EnumArgumentType` if it mirrors the option variants. `is_option()` returns `true`.
`liquers-core/src/plan.rs` `ParameterValue::from_string`: a `BooleanOption` arm. Empty → the
argument's default, else `Value::Null` (as `IntegerOption`); lower-cased `t|true|yes|y|1` →
`Bool(true)`, `f|false|no|n|0` → `Bool(false)`, `none` → `Value::Null`; else
`Error::conversion_error_at_position(s, "boolean or none", pos)`. Fix every exhaustive match on
`ArgumentType` (`liquers-lib/src/commands.rs` ≈89 "Boolean?", `liquers-lib/src/egui/widgets.rs`
≈653 "bool?", `liquers-py/src/command_metadata.rs`, and any the compiler names). GUI default: the
same widget as `Boolean` (a three-state widget is out of scope).

## Macro

`liquers-macro/src/registration.rs`:

- Fix `ArgumentType::FloatOpt` → `FloatOption` (two arms and the test at ≈2278).
- Map every supported `Option<T>`: integer widths → `IntegerOption`, `f32`/`f64` → `FloatOption`,
  `bool` → `BooleanOption`.
- One `SUPPORTED_OPTION_INNER` list, commented to point at the core impls. At parse time (so the
  span is the user's type), an `Option<T>` with `T` outside the list returns
  `syn::Error::new_spanned(ty, "register_command!: argument '{name}' has type Option<{T}>, which is
  not supported; supported: Option<{list}>. For an optional string, use '{name}: String = \"\"'")`.
  This includes `Option<String>`, `Option<Value>` and non-ident inners (`Option<Vec<u8>>`).

## Alternatives

`StringOption` / `Option<Value>` support: rejected by the maintainer. Reusing `Boolean` for
`Option<bool>`: rejected, `""` would mean `false`. A blanket `TryFrom` impl: rejected (coherence).

## Known-issue preflight

`command-metadata-descriptions-and-hints` also edits `registration.rs` argument parsing: different
arms; either order. No blocker.

## Commands and documents

No command changes, so `specs/command_registry.yaml` is unchanged (no in-tree command uses an
option type; verify with the registry test). Documents: `REGISTER_COMMAND_FSD.md` (supported
`Option<T>`, `bool_opt` spelling, the error), `COMMAND_REGISTRATION_GUIDE.md` (one example),
`CLAUDE.md` DSL lines if they list types; History rows, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | core `value.rs`, `commands.rs`, `command_metadata.rs`, `plan.rs`; macro `registration.rs`; lib `value/simple.rs`, `value/extended.rs`, `commands.rs`, `egui/widgets.rs`; py `command_metadata.rs` |
| Existing tests | The macro unit test expecting `FloatOpt` changes; all registrations must expand |
| Compatibility | New wire value `bool_opt`; existing values unchanged |
| Feature matrix | `egui` match is feature-gated: run `scripts/check-build-matrix.sh` |
| Recovery | Revert; nothing stored depends on it |
| Certainty | High (probe compile confirmed the missing bound, 2026-10-10) |

## Review findings to resolve before implementation (2026-10-10)

Raised by an automated review of the design PR (#104). The first was checked against the code;
the others are plausible and must be checked at step 1.

1. **`TryFrom<Value> for bool` rejects `Value::Bool`** (checked: `liquers-core/src/value.rs`
   matches only `I32` / `I64`). A linked `Option<bool>` argument delegating to `bool::try_from`
   would fail on a real boolean. Add the `Bool` arm and a link-to-boolean test.
2. **An omitted optional argument may never reach `from_string`.** For an argument without a
   default the macro emits `CommandParameterValue::None`, and the plan reports a missing action
   parameter as `ArgumentMissing` (`plan.rs`) before any parsing. The bare `opt_i` / `opt_b` examples
   in Phase 3 need the plan to emit a null default for `Option<T>`, or to synthesize null for an
   optional argument type.
3. **`SimpleValue` / `CombinedValue` lack scalar conversions** for some promised widths (`i8`,
   `i16`, `isize`, `u16`, `u64`, `usize`). Add them, implement the option conversions directly, or
   narrow the supported list.
