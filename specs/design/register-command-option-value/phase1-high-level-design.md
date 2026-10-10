# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds `TryFrom`/`FromParameterValue` impls in `liquers-core`,
  an `ArgumentType` variant and macro diagnostics in `liquers-macro` (rules 3, 4, 6)
- **Leading issue:** **Open design question — scope after the 2026-10-10 finding** (below).
- **Explanation:** Maintainer decisions of 2026-10-10 fix the semantics; the remaining question is
  whether the numeric fix and `Option<bool>` share this design (re-sized `M`).
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
- **Open questions:**
  1. **Proposed resolution — one design, re-sized `M`:** fix numeric options (the `TryFrom` impls
     for `Value` in `liquers-core` and `ExtValue` in `liquers-lib`, `FloatOpt` → `FloatOption`, the
     missing integer widths), add `Option<bool>` with `ArgumentType::BooleanOption`, and reject every
     other `Option<T>` with the clear error.

## Problem

`register_command!` accepts any `Option<T>`. For a `T` outside the numeric set, the generated
`arguments.get::<Option<T>>(i, name)` fails to compile with an `E0277` trait-bound error deep in
macro output (`CommandArguments::get` requires `FromParameterValue<T> + TryFrom<E::Value>`).

## Expected behaviour and acceptance

1. `fn f(state, x: Option<Value>) -> result` fails at expansion with a message naming the
   parameter, the type, the supported list, and the workaround.
2. All existing commands compile unchanged.
3. (If question 2 is accepted) `Option<String>` and `Option<bool>` work: an absent parameter or a
   JSON `null` gives `None`, otherwise `Some`. Their `ArgumentType` is decided below.

## Scope

Macro validation, and possibly two core impls. No `Option<Value>` support.

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
- `ArgumentType` for `Option<String>`: there is no `StringOption` variant. The macro falls back to
  `ArgumentType::Any` for unknown inners today. Recommended: map `Option<String>` → `String` and
  `Option<bool>` → `Boolean`, and record `None` as the default value. Check how
  `IntegerOption` differs from `Integer` in validation before finalizing. If the distinction
  matters there, defer `Option<String>`/`Option<bool>` and only reject.
