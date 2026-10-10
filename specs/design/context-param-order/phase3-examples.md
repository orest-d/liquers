# Phase 3: Examples and Tests — Context parameter at any position

## Overview Table

| Example / test group | Kind | Where | Scenarios |
|---|---|---|---|
| Problem example, context-position end-to-end | runnable integration | `liquers-core/tests/context_parameter_position.rs` | AC-1, AC-2, AC-3, AC-5, AC-9 |
| Signature diagnostics and call order | runnable unit | `liquers-macro/src/registration.rs` `mod tests` | AC-1, AC-2, AC-4, AC-5, AC-6, AC-11 |
| Argument numbers in runtime errors | runnable unit | `liquers-core/src/commands.rs` `mod argument_number_tests` | AC-12 |
| Existing commands, documents, test hygiene, warnings | check commands | Phase 4 validation | AC-7, AC-8, AC-9, AC-10, AC-11 |

Every query below was checked with `liquers-validate --command …`: `data/load-5`, `data/f-5`,
`data/g-1-x`, `h-5`, `f-abc` and `data/m-a-b` each parse as `data` (where present) followed by one
action with the written parameters.

## Example

**Primary — the problem example, end to end** (AC-1). In `context_parameter_position.rs`, with
`type CommandEnvironment = SimpleEnvironment<Value>` as in `async_hellow_world.rs`:

```rust
fn data(_state: &State<Value>) -> Result<Value, Error> { Ok(Value::from("x")) }
async fn load(context: Context<CommandEnvironment>, state: State<Value>, limit: i64)
    -> Result<Value, Error> {
    context.info("loading")?;
    Ok(Value::from(format!("{}:{limit}", state.try_into_string()?)))
}
register_command!(cr, fn data(state) -> result)?;
register_command!(cr, async fn load(context, state, limit: i64) -> result)?;
// metadata: exactly one argument, `limit`
let state = evaluate(env.to_ref(), "data/load-5", None).await?;
assert_eq!(state.try_into_string()?, "x:5");
```

Compiling at all is half the proof: had the wrapper passed the state first, `load` would receive a
`State` where it declares a `Context` and the expansion would not type-check.

**Secondary — the diagnostics** (AC-4, AC-6, AC-11), as `syn::Error` text from parsing a
`CommandSignature`:

| Signature | Error, at the token shown |
|---|---|
| `fn t(state, context, a: i64, Context)` | at `Context`: `` `context` is declared twice `` |
| `fn t(a: i64, state)` | at `state`: `` the state parameter `state` must come before every argument; only `context` may precede it `` |
| `fn t(value, text)` | at `text`: `the state parameter is declared twice` |
| `fn t(state, context: i64)` | at `context`: `` `context` is reserved for the execution context and takes no type `` |
| `fn t(state, a: i64 (hint icon: "x"))` | at `hint`: `` argument hints are not supported; `hint` would be ignored `` |

**Runtime numbering** (AC-12): `CommandArguments::get::<i64>(0, "limit")` on an empty parameter
list fails with `Missing argument #1 'limit'`.

## Edge and Error Cases

| Case | Expected | Proved by |
|---|---|---|
| `fn f()` and `fn f(state, a: i64,)` (empty list, trailing comma) | parse as today | `signature_empty_and_trailing_comma_still_parse` |
| `fn f(context)` alone | `nostate(context)`, no arguments | `test_nostate_command_registration2` (exists) |
| `fn f(context, text)` | text state passed second as `state.try_into_string()?.as_str()` | `signature_context_before_state_calls_in_declared_order` |
| `context` before a `multiple` argument (the recommended form) | compiles; `data/m-a-b` yields both elements | `context_before_multiple_argument` |
| `context` after a `multiple` argument | still valid (variadic rule unchanged) | `extract_all_parameters_with_context_last` (exists) |
| An injected argument before `context` | slot numbering unchanged | `extract_all_parameters_with_context_middle_and_injected` (exists) |
| Argument named `state` with a type, after the state | an argument | `signature_bare_keyword_is_state_typed_keyword_is_argument` |

## Test Plan

**`liquers-core/tests/context_parameter_position.rs`** (new; no feature gate, core only):

| Test | Proves |
|---|---|
| `context_before_state_async_problem_example` | AC-1 — the primary example above |
| `context_before_state_sync` | AC-1 — `fn f(context, state, n: i64)`; `data/f-5` → `"x:5"`; metadata argument names `["n"]` |
| `context_between_arguments_with_injected` | AC-2 — `fn g(state, a: i64, context, b: Marker injected, c: String)`, `Marker` a local `InjectedFromContext` type; `data/g-1-x` → `"x:1:marker:x"` |
| `context_first_without_state` | AC-3 — `fn h(context, n: i64)`; `h-5` → `"5"` |
| `stateless_argument_named_value` | AC-5 — `fn f(value: String)`; `f-abc` → `"abc"`; metadata argument `value` |
| `context_before_multiple_argument` | AC-9 — `fn m(state, context, xs: Vec<String> multiple)`; `data/m-a-b` → `"x:a,b"` |

**`liquers-macro/src/registration.rs` `mod tests`** (unit; parse with `syn::parse2` and inspect the
`syn::Error` message or the generated tokens):

| Test | Proves |
|---|---|
| `signature_context_before_state_calls_in_declared_order` | AC-1 — wrapper arguments `context, state, n__par`, and the text-state form |
| `signature_context_between_arguments_calls_in_declared_order` | AC-2 — `state, a__par, context, b__par, c__par` |
| `signature_context_twice_is_rejected` | AC-4 — both `context, context` and `context, Context` |
| `signature_bare_keyword_is_state_typed_keyword_is_argument` | AC-5 — `fn t(value: String)` has no state and one argument; `fn t(value)` has a value state |
| `signature_state_after_argument_is_rejected` | AC-6 |
| `signature_state_twice_is_rejected` | AC-6 |
| `signature_context_with_type_is_rejected` | the `context: T` diagnostic (Phase 2) |
| `signature_empty_and_trailing_comma_still_parse` | AC-7 — the parser rewrite keeps both forms |
| `argument_hint_option_is_rejected` | AC-11 |
| `test_nostate_command_registration{1,2}`, `test_config_command_registration`, `test_sync_command_does_not_set_is_async_flag` | AC-10 (rewritten in `a849a47`) |

**`liquers-core/src/commands.rs` `mod argument_number_tests`** (unit; `CommandArguments::<SimpleEnvironment<Value>>::new` over a hand-built `ResolvedParameterValues`):

| Test | Proves |
|---|---|
| `missing_argument_is_numbered_from_one` | AC-12 — `Missing argument #1 'limit'` |
| `unresolved_link_names_argument_from_one` | AC-12 — `DefaultLink` in slot 1 → `argument #2 'path'` |
| `unresolved_placeholder_names_argument_from_one` | AC-12 — `Placeholder` via `get_value` |
| `non_list_multiple_argument_is_numbered_from_one` | AC-12 — `get_multiple` on a `DefaultValue` |

**Check commands** (run in Phase 4 validation):

| Check | Proves |
|---|---|
| `cargo test -p liquers-lib --lib --tests` and `cargo test -p liquers-lib --test registry_export` | AC-7 — every macro use compiles; registry unchanged |
| `grep -rniE "context.{0,40}(must\|should\|has to) (be\|come) (the )?last"` over the AC-8 documents returns nothing; the FSD and guide contain the AC-9 recommendation | AC-8, AC-9 |
| `grep -c 'println!' liquers-macro/src/registration.rs` is 0 | AC-10 |
| `cargo build -p liquers-macro` prints no `warning:` | AC-11 |

Run: `cargo test -p liquers-macro`, `cargo test -p liquers-core --lib argument_number_tests`,
`cargo test -p liquers-core --test context_parameter_position`.
