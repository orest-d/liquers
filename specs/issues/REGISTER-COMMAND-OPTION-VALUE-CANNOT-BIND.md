---
id: REGISTER-COMMAND-OPTION-VALUE-CANNOT-BIND
kind: issue
title: register_command! cannot bind an Option<Value> argument
status: draft
priority: P3
complexity: S
area: [macro]
design: register-command-option-value
created: 2026-09-26
github:
---

## Problem

`register_command!`'s DSL accepts `Option<i32>`/`Option<i64>`/`Option<f32>`/`Option<f64>` as
argument types (`liquers-macro/src/registration.rs`'s `argument_type_expression`, generating
`ArgumentType::IntegerOption`/`FloatOption`), but there is no `FromParameterValue<Option<Value>>`
or `TryFrom<E::Value>` impl for `Option<Value>` in `liquers-core/src/commands.rs`. A parameter
declared `schema: Option<Value>` compiles at the macro level (its `argument_type_expression` falls
through to `ArgumentType::Any` because the inner type name `"Value"` is not in the `(bool,
Option<&str>)` match), but the generated wrapper's `arguments.get::<Option<Value>>(i, name)` call
fails to compile: `CommandArguments::get`'s bound is
`T: FromParameterValue<T> + TryFrom<E::Value, Error = Error>`, and neither trait is implemented for
`Option<Value>` (or `Option<V>` for a generic `V: ValueInterface`) anywhere in the codebase.

Discovered designing `ns-rec`'s `schema` argument (record-streams Phase 4 Step 5.5): Phase 2 left
"how the macro spells an optional value-typed argument" as a Phase 4 detail, and `Option<Value>`
was the first spelling tried, per the plan's own instruction to check it before falling back to
`schema: String = ""`.

## Impact

Nobody can declare a command argument that is "a `Value`, or absent" through `register_command!`
today. The workaround — a `String` argument, empty meaning absent, non-empty meaning "parse this
text as the intended shape" (YAML/JSON, in `ns-rec`'s case) — works and is what `ns-rec/to_record`,
`ns-rec/from_json` and `ns-rec/records_schema` use, but it pushes the parsing back into every
command's own body and loses the type checking a real `Option<Value>` argument would have given at
metadata-validation time. No known caller is blocked today; this is a gap in the DSL's expressivity
rather than a broken feature.

## Expected behaviour

One of:
- `register_command!` rejects `Option<Value>` (and `Option<T>` for any non-numeric `T`) at macro
  expansion time with a clear message, rather than letting it reach a confusing `E0277` at the
  generated wrapper's `arguments.get::<Option<Value>>(...)` call; or
- `liquers-core::commands` grows `FromParameterValue<Option<V>>`/`TryFrom<E::Value> for Option<V>`
  for `V: ValueInterface`, treating a JSON `null` (or an absent/default parameter) as `None` and
  anything else as `Some(V::try_from_json_value(...))`, making `Option<Value>` a real, working
  argument spelling.

The second is more useful but is real work (`get`'s existing `T::try_from(E::Value)` fast path for
a pre-materialized link value also needs an `Option<V>` case) — a human should decide which is
worth doing before either is implemented.

## Discovery

Verified against `liquers-core/src/commands.rs`'s `impl_from_parameter_value2*!` macro invocations
(only numeric types get an `_opt` variant) and `liquers-macro/src/registration.rs`'s
`argument_type_expression` (its `is_option_of` match has no `(true, Some("Value"))` arm) while
settling `ns-rec`'s `schema` argument spelling in record-streams Phase 4 Step 5.5. Not compiled as
a standalone repro; the reasoning above is read directly off both files' current source.
