# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — adds `TryFrom` impls on `pub` types in `liquers-core` and
  `liquers-lib`, a serialized `ArgumentType::BooleanOption` variant, and macro diagnostics, across
  core, macro, lib and py (rules 3, 4, 6)
- **Leading issue:** None
- **Explanation:** All questions decided on 2026-10-10. Re-sized from `S` to `M` by maintainer
  decision: one design fixes numeric options, adds `Option<bool>`, and rejects every other
  `Option<T>`.
- **Decided (Maintainer decision, 2026-10-10):**
  1. `Option<String>` and `Option<Value>` are **not** supported, and there are no plans to: there is
     no easy way to spell `None` for a string in a query (entities could, later), and `Value` can
     already be none. `register_command!` rejects them with a clear expansion-time error naming the
     parameter, the supported types and the `String = ""` workaround. Limits on argument types in
     the query are acceptable.
  2. `Option<bool>` **should** be supported. Query text, case-insensitive: `t`, `true`, `yes`, `y`,
     `1` → `Some(true)`; `f`, `false`, `no`, `n`, `0` → `Some(false)`; `none` → `None`. The empty
     string means the declared default, or `None` when no default is declared.
- **Finding (2026-10-10, verified by compiling a probe registration):** no `Option<T>` argument
  works today, numeric ones included. `register_command!(cr, fn f(state, n: Option<i64>) -> result)`
  fails with `E0277`/`E0271`: `CommandArguments::get` requires `TryFrom<E::Value, Error = Error>`,
  and there is no `TryFrom<Value> for Option<T>` for any `T`. The macro also emits the non-existent
  `ArgumentType::FloatOpt` for `Option<f32>`/`Option<f64>` (the variant is `FloatOption`), and maps
  `Option<i8|i16|u8|u16|isize>` to `Any`. The `FromParameterValue<Option<T>>` impls and
  `ArgumentType::IntegerOption`/`FloatOption` parsing exist; only these links are missing.
  `Option<bool>` needs the same `TryFrom` impls plus a new `ArgumentType::BooleanOption` (parsing as
  decided above), so nothing fundamental prevents it.
- **Scope (Maintainer decision, 2026-10-10):** one design, re-sized `M`: fix numeric options (the
  `TryFrom` impls, `FloatOpt` → `FloatOption`, the missing integer widths), add `Option<bool>` with
  `ArgumentType::BooleanOption`, and reject every other `Option<T>` with the clear error.
- **Open questions:** None.

## Problem

`register_command!` accepts any `Option<T>`. For a `T` outside the numeric set, the generated
`arguments.get::<Option<T>>(i, name)` fails to compile with an `E0277` trait-bound error deep in
macro output (`CommandArguments::get` requires `FromParameterValue<T> + TryFrom<E::Value>`).

## Expected behaviour and acceptance

1. `fn f(state, n: Option<i64>)` (and every integer width, `f32`, `f64`) registers, and evaluates:
   `f` → `None`, `f-5` → `Some(5)`, a declared default applies on `""`.
2. `fn g(state, flag: Option<bool>)` registers with `ArgumentType::BooleanOption` (wire `bool_opt`):
   `g-t`/`g-TRUE`/`g-yes`/`g-y`/`g-1` → `Some(true)`; `g-f`/`g-no`/`g-n`/`g-0` → `Some(false)`;
   `g-none`/`g-NONE` → `None`; `g` (empty) → the default, else `None`; anything else → conversion
   error at the parameter position.
3. A link argument (`~X~…~E`) resolving to a none value gives `None`, otherwise the converted value.
4. `fn h(state, x: Option<Value>)`, `Option<String>` or any other `Option<T>` fails at expansion with
   a message naming the parameter, the type, the supported list and the `String = ""` workaround.
5. All existing registrations compile unchanged.

## Scope

`liquers-core` (`commands.rs`, `value.rs`, `command_metadata.rs`, `plan.rs`), `liquers-macro`
(`registration.rs`), `liquers-lib` (`TryFrom` for `SimpleValue`/`CombinedValue`, two display
matches), `liquers-py` (the mirrored enum). No `Option<String>`/`Option<Value>` support.

## Design Dependencies

None. (`record-streams`, complete, used the `String = ""` workaround in `ns-rec`.)

## Documentation assessment

- Reference: `specs/reference/REGISTER_COMMAND_FSD.md`: list the supported `Option<T>` types and
  the error.
- Guide: `COMMAND_REGISTRATION_GUIDE.md`: one sentence and the workaround.

## Consolidated Findings

- The supported set must be derived from the core impls. Keep a single list in the macro with a
  comment pointing to `impl_from_parameter_value2_opt!` in `liquers-core/src/commands.rs`. A test
  per spelling (compiling registrations) keeps the two in step.
- `IntegerOption` differs from `Integer` exactly where it matters: an empty parameter becomes
  `null` (→ `None`) instead of an error. `Boolean` maps empty to `false`, so `Option<bool>` cannot
  reuse it; hence `BooleanOption`. A `StringOption` would have no spelling for `None`, which is why
  `Option<String>` is rejected.
