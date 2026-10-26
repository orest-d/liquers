# Phase 3: Examples & Use-cases — a generic command dispatches by its input's type

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | `head` over frames and record views | both selection times | AC-1, AC-2 |
| 2 | Example | Registering a generic | `register_generic` | AC-5 |
| 3 | Test | `liquers-core/tests/generic_commands.rs::run_time_dispatch_logs_choice` | `Dispatch` evaluated | AC-1 |
| 4 | Test | `…::plan_time_specialization_sets_origin` | `Specialized` origin, no `Dispatch` | AC-2 |
| 5 | Test | `…::selection_precedence` | principal, order, fallback | AC-3 |
| 6 | Test | `…::no_variant_is_a_conversion_error` | message, type, position | AC-4 |
| 7 | Test | `…::register_generic_refuses_bad_variants` | each rule, nothing registered | AC-5 |
| 8 | Test | `…::dispatch_flags_and_dependencies_are_conservative` | volatile variant, scan keys | AC-6 |
| 9 | Test | `liquers_core::command_metadata::tests::generic_definition_round_trips` | JSON/YAML; plan with `Dispatch` | AC-7 |
| 10 | Test | `liquers-lib/tests/generic_head.rs::head_on_csv_and_record_view` | shipped `head` (gated polars+records) | AC-1, AC-2 |
| 11 | Test | `…::namespaced_head_unchanged` | `ns-pl/head-5` plan identical | AC-8 |
| 12 | Test | `liquers-lib/tests/registry_export.rs` (existing) and `liquers-validate` | export; problem example validates | AC-7 |

## Example 1: `head` over frames and record views

```
-R/data/sales.csv/-/head-5                               → Dispatch[pl/head, rec/head]; runs pl/head
-R/data/sales.csv/-/ns-rec/to_record/ns-root/head-5      → Action rec/head, origin Specialized{head}
-R/data/sales.csv/-/ns-pl/head-5                         → Action pl/slice, origin Alias{pl/head} (unchanged)
```

(All three parse and plan today with `liquers-validate --command head`; the first two fail without
it.) The first query's log has `dispatched 'head' to 'pl/head' for Bytes` (or `Text`).

## Example 2: registering a generic

Core test setup (`SimpleEnvironment<Value>`, memory store): `describe_text(text) produces Text`
declared `accepts: "Text"`; `describe_number(state)` declared `accepts: ["I64", "I32"]`;
`describe_any(state)` undeclared; `make_i32-n` declared `produces: "I32"`.

```rust
cr.register_generic(CommandKey::new("", "", "describe"),
    vec![key("describe_text"), key("describe_number"), key("describe_any")])?;
// make_i32-7/describe      → plan-time: Specialized → describe_number (I32 accepted, order)
// -R/x.txt/-/describe      → run-time: a stored file read as Text → describe_text (principal)
// -R/x.bin/-/describe      → run-time: a stored file read as Bytes → describe_any (fallback)
```

Without `describe_any`, the third fails with `no variant of 'describe' accepts 'Bytes'; variants:
describe_text (Text), describe_number (I64, I32)`.

## Edge and Error Cases

- Two variants with principal `Text`, two undeclared variants, a variant that is a generic, a source
  command, an unknown key, another realm, or arguments differing in count/name/type/flags:
  `register_generic` errors (test 7).
- A flowing type with two identifiers selecting different variants: `Dispatch`, not a guess.
- A `Dispatch` as the last step of a recipe with an argument override: the override is applied to
  every variant step (test 8 also checks `override_value` returns `true`).
- A variant alias whose target is volatile: the dispatch plan is volatile (test 8).
- An alias whose target is a generic: refused at registration and at planning.
- An input in error status never reaches `Dispatch` (`do_step_state` stops it first).

## Test Plan

- `cargo test -p liquers-core --test generic_commands` (tests 3-8) and `--lib command_metadata` (9).
- `cargo test -p liquers-lib --test generic_head --test registry_export` (10-12), then
  `./target/debug/liquers-validate -- '-R/data/sales.csv/-/head-5'` after rebuilding.
- `cargo test -p liquers-mcp`; `bash scripts/check-build-matrix.sh`; `bash scripts/test-liquers-py.sh`.
