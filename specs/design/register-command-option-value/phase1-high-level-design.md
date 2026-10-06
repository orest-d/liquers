# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — reject `Option<Value>` or support it.** The issue
  asks a human to choose. Supporting it is real work in `liquers-core::commands`
  (`FromParameterValue<Option<V>>`, `TryFrom<E::Value> for Option<V>`, the link-value fast path)
  and defines new semantics (what `null`/absent mean).
- **Explanation:** The design specifies the recommended, smaller answer: a clear expansion-time
  error for every unsupported `Option<T>`. Support is left as a separately filed feature.
- **Open questions:**
  1. **Proposed resolution — reject with a compile error now.** `register_command!` emits
     `compile_error!`-equivalent `syn::Error` on an `Option<T>` whose `T` has no
     `FromParameterValue<Option<T>>` impl, naming the supported spellings and the
     `String = ""` workaround. Today's outcome is a confusing `E0277` in generated code.
  2. **Open design question — supported set.** Today `Option<i8…i64, isize, u8…u64, usize, f32,
     f64>` have impls (`impl_from_parameter_value2_opt!` plus explicit `Option<i64>`/`Option<f64>`).
     The macro must allow exactly these. `Option<bool>` and `Option<String>` have no impls and
     would be rejected too. Should they instead get impls in this change (two lines each, mirroring
     the numeric ones)? Recommended: yes for `Option<String>` and `Option<bool>`, which are cheap
     and unambiguous (JSON `null` → `None`). `Option<Value>` stays rejected.

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
