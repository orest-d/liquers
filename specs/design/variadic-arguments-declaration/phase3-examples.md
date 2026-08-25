# Phase 3: Examples & Use-cases - Declarable variadic command arguments

## Introduction

Phase 1 set the purpose: make a mechanism that already works reachable by command authors. So these
examples are mostly *author-facing* — what you write in `register_command!` and what the compiler
says when you write it wrong — with one caller-facing thread showing what the queries then mean.

The progression is: Example 1 converts `pl/select_columns`, the command the issue named, and shows
the whole path from declaration to query. Example 2 goes past strings to a numeric variadic and to
links, which is where the element-type decision (Phase 1 D3) earns itself. Example 3 is the six
ways to declare it wrong and what each says.

**Examples are runnable and test-shaped**, per the Phase 3 question: every snippet below is written
so it compiles and passes once Phase 4 lands, and the test sections are the Phase 4 suite rather
than a sketch of it.

### Evidence already in hand

The plan-level behaviour is not predicted here — it is **measured**, before any Rust is written, by
running `liquers-validate` against a registry overlay declaring the proposed signatures
(`--registry-file specs/command_registry.yaml --registry-file <proposal>.yaml --allow-overwrite`,
the workflow `CLAUDE.md` prescribes for a design that changes an existing signature). The overlay is
kept beside this document as [`variadic-proposal.registry.yaml`](./variadic-proposal.registry.yaml)
so the measurement can be reproduced:

```bash
cargo run -p liquers-core --features cli --bin liquers-validate -- \
  --registry-file specs/command_registry.yaml \
  --registry-file specs/design/variadic-arguments-declaration/variadic-proposal.registry.yaml \
  --allow-overwrite -- 'ns-pl/select_columns-a-b-c'
```

| Query | Resolved parameters |
|---|---|
| `ns-pl/select_columns-a-b-c` | `MultipleParameters[ ParameterValue(columns,"a")@22, ParameterValue(columns,"b")@24, ParameterValue(columns,"c")@26 ]` |
| `ns-pl/select_columns-a~_b` | `MultipleParameters[ ParameterValue(columns,"a-b")@22 ]` |
| `ns-pl/select_columns` | `MultipleParameters[ ]` |
| `ns-pl/select_columns-a-b-c` *without* the overlay (HEAD today) | `TooManyParameters: accepts 1, but parameter #2 'b' was supplied` @ offset 23 |

Three things this settles, which the rest of the document relies on rather than re-argues:

1. Three parameters become three elements — the mechanism needs no change, only a declaration.
2. `a~_b` becomes **one** element `"a-b"`. The dash-containing column name is expressible, and
   distinguishable from two columns. Under today's `split('-')` it is not.
3. No parameters becomes the **empty** list, not an error — confirming Phase 1 D4 and confirming the
   command must reject emptiness itself.

Every element carries its own `Position`. That is what makes the per-element conversion errors in
`get_multiple` point at the offending parameter rather than at the action.

## Overview Table

| # | Item | Kind | Demonstrates / checks |
|---|---|---|---|
| E1 | Converting `pl/select_columns` | Example | The whole path: declaration, function signature, deleted workaround, resulting queries |
| E2 | A numeric variadic and a linked element | Example | Why `ArgumentType` must follow the element type; links inside a variadic resolve element-wise |
| E3 | Six ways to get the declaration wrong | Example | Each compile-time rejection and its message |
| U1 | `get_multiple` over a three-element list | Unit | Happy path, order preserved |
| U2 | `get_multiple` over an empty list | Unit | Empty vector, not an error |
| U3 | `get_multiple` on a non-variadic slot | Unit | Declaration/retrieval mismatch is reported, not mis-converted |
| U4 | `get_multiple` on an unresolved link element | Unit | Positioned error naming the link |
| U5 | `get_multiple::<i64>` conversion failure | Unit | Error carries the *element's* position, not the action's |
| U6 | `get_multiple::<i64>` happy path | Unit | Non-string element types work |
| U7 | Macro: `multiple` sets `ArgumentInfo.multiple` | Unit | Generated metadata |
| U8 | Macro: `Vec<String>` → `ArgumentType::String` | Unit | Element-type inference |
| U9 | Macro: emits `get_multiple`, not `get` | Unit | Correct accessor chosen |
| U10 | Macro: existing declarations expand unchanged | Unit | No regression in the non-variadic path |
| U11-U16 | Macro: the six rejections | Unit | Each declaration fails to parse, with the right message |
| U17 | Ordering rule exempts `injected` and `context` | Unit | A variadic followed by injected/context still parses |
| I1 | `ns-pl/select_columns-a-b` end to end | Integration | Two columns selected, through a real query |
| I2 | `ns-pl/select_columns-a~_b` end to end | Integration | One column named `a-b`; the new capability |
| I3 | `ns-pl/select_columns` (empty) end to end | Integration | Command rejects the empty list with a clear message |
| I4 | `ns-pl/drop_columns-b` end to end | Integration | The second converted command |
| I5 | Unknown column in a variadic list | Integration | Existing `check_column_exists` still fires, per element |
| I6 | Registry round-trip of `multiple` + `argument_type` | Integration | The serde combination that has never existed |
| I7 | `registry_export` agreement | Integration | Regenerated `command_registry.yaml` matches the code |
| C1-C4 | Corner cases | Integration | See "Corner cases" |

## Example 1 — Converting `pl/select_columns`

The primary use case, and the one the issue is about. Three edits.

**The declaration** (`liquers-lib/src/polars/selection.rs:105`). One word changes in the DSL, plus
a `doc:` that now describes behaviour instead of a workaround:

```rust
register_command!($cr,
    fn select_columns(state, columns: Vec<String> multiple) -> result
    namespace: "pl"
    label: "Select columns"
    doc: "Select columns by name, one parameter per column"
    version: auto
)?;
```

`multiple` sits exactly where `injected` sits — after the type, before any default or metadata
parens. The two are mutually exclusive, so at most one ever appears.

**The function.** The signature takes the container; the body loses the workaround:

```rust
/// Select specific columns by name.
///
/// Arguments:
/// - columns: one column name per parameter (`select_columns-date-amount`).
///   A name containing a dash is escaped: `select_columns-a~_b` selects the single column `a-b`.
#[liquers_macro::command_version]
pub fn select_columns(state: &State<Value>, columns: Vec<String>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    if columns.is_empty() {
        return Err(Error::general_error(
            "select_columns requires at least one column name".to_string(),
        ));
    }

    for col in &columns {
        check_column_exists(&df, col)?;
    }

    let result = df
        .select(&columns)
        .map_err(|e| Error::general_error(format!("Failed to select columns: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}
```

Gone: `columns.split('-')` and the `.map(|s| s.trim())` that existed only to clean up after it.
Both were workarounds; keeping either would make `a~_b` and `a-b` work by two different mechanisms
and would keep mangling a column name that genuinely contains a dash.

New: the empty-list rejection. The plan builder cannot supply it — a variadic argument with no
parameters is legitimately empty (measured above), and only the command knows it needs at least one.

**What the caller writes.** The escaped spelling that
`design/excess-action-parameters-error/` introduced as a workaround is now the way to say something
different:

```
ns-pl/select_columns-date-amount-status     three columns
ns-pl/select_columns-a~_b                   one column, named "a-b"
```

Both validate today against the proposal overlay; the first is an error at HEAD.

## Example 2 — Beyond strings: numeric elements and links

Builds on Example 1 rather than repeating it. Two mechanisms that the string case does not exercise.

**Element type drives parsing, not just metadata.** Suppose a command taking a list of row indices:

```rust
fn pick_rows(state: &State<Value>, rows: Vec<i64>) -> Result<Value, Error> { /* … */ }

register_command!(cr, fn pick_rows(state, rows: Vec<i64> multiple) -> result namespace: "pl")?;
```

The macro infers `ArgumentType::Integer` from the **element** type `i64`. This is not cosmetic.
`pop_value`'s variadic branch converts each action parameter through
`ParameterValue::from_string(arginfo, s, pos)` (`plan.rs:741`), which dispatches on
`arginfo.argument_type`:

| Inferred `ArgumentType` | `pick_rows-1-2-3` resolves to | `get_multiple::<i64>` |
|---|---|---|
| `Integer` (this design) | `Value::Number(1)`, `Number(2)`, `Number(3)` | three `i64`s |
| `Any` (what `Vec<i64>` gets today) | `Value::String("1")`, … | **fails** — `p.as_i64()` on a JSON string returns `None` |

So without Phase 1 D3, every non-string variadic argument would be declarable and then fail at
retrieval. It also means a bad element is caught at *plan* time with a position:
`pick_rows-1-x-3` fails at `x`, before the command runs.

**Links inside a variadic resolve element-wise.** Nothing extra is needed for this — it is why
Phase 2 adds no async surface:

```
ns-pl/select_columns-a-~X~-R/config/colname.txt~E-c
```

`pop_value` stores a `ParameterLink` for the linked element (`plan.rs:777-783`); the interpreter's
`materialize_nested_parameter` (`interpreter.rs:344`) walks `MultipleParameters` and replaces each
link with its resolved value **before** `CommandArguments` is constructed
(`interpreter.rs:462-466`). By the time `get_multiple` runs, every element is a value. Dependency
collection already recurses the same way (`interpreter.rs:115`).

That ordering is exactly what lets `get_multiple` be a sync, non-`E`-using function.

## Example 3 — Getting the declaration wrong

The compiler messages, since they are the only feedback a macro author gets. Each is a
`syn::Error` with a span, surfaced by `parse_macro_input!` (`registration.rs:1804`) as a
`compile_error!` at the offending token.

| What you write | What you get |
|---|---|
| `columns: String multiple` | ``a `multiple` argument must have a container type; `String` is not one. Expected `Vec<String>` `` |
| `columns: Vec<String> multipel` | ``unknown argument flag `multipel`; expected `injected` or `multiple` `` |
| `columns: Vec<String> injected multiple` | ``an argument cannot be both `injected` and `multiple`: an injected argument consumes no query parameters`` |
| `columns: Vec<String> multiple multiple` | ``duplicate argument flag `multiple` `` |
| `columns: Vec<String> multiple = "x"` | ``a `multiple` argument cannot have a default value; it defaults to the empty list`` |
| `fn f(state, a: Vec<String> multiple, b: i32)` | ``argument `b` follows the `multiple` argument `a` and can never receive a value`` |

The first is the mistake most likely to happen — `columns: String multiple` is the natural
half-conversion — which is why the message renders the declared type and names the fix.

The second is the one that must exist *before* any of the others are useful. Today the parser
accepts any identifier and compares it to `"injected"` (`registration.rs:1564`), so `multipel`
would be silently discarded and the argument would be a plain scalar. Adding a second flag without
fixing that turns every typo into a silent behaviour change.

The last is `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`, caught at compile time. It reports on
`b` — the starved argument — because that is where the author must edit.

## Test Plan

### Unit tests

#### liquers-core — `get_multiple` (`liquers-core/src/commands.rs`, `#[cfg(test)] mod tests`)

Environment: none needed. These construct `CommandArguments` directly, which is the point — they
also cover the paths the interpreter does not take.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::SimpleEnvironment;
    use crate::plan::{ParameterValue, ResolvedParameterValues};
    use crate::query::Position;
    use crate::value::Value;

    // `commands.rs`'s own test modules already alias this (`:733`).
    type TestEnv = SimpleEnvironment<Value>;

    fn value_element(name: &str, v: serde_json::Value, offset: usize) -> ParameterValue {
        ParameterValue::ParameterValue(name.to_string(), v, Position::new(offset, 1, offset + 1))
    }

    /// U1 - three elements convert in order.
    #[test]
    fn get_multiple_returns_elements_in_order() -> Result<(), Error> {
        let params = ResolvedParameterValues(vec![ParameterValue::MultipleParameters(vec![
            value_element("columns", "a".into(), 21),
            value_element("columns", "b".into(), 23),
            value_element("columns", "c".into(), 25),
        ])]);
        let args = CommandArguments::<TestEnv>::new(params);

        let columns: Vec<String> = args.get_multiple(0, "columns")?;
        assert_eq!(columns, vec!["a", "b", "c"]);
        Ok(())
    }

    /// U2 - an empty variadic argument is an empty vector, NOT an error. This is the
    /// `select_columns` (no parameters) case, which the plan builder produces legitimately.
    #[test]
    fn get_multiple_empty_list_is_ok() -> Result<(), Error> {
        let params =
            ResolvedParameterValues(vec![ParameterValue::MultipleParameters(Vec::new())]);
        let args = CommandArguments::<TestEnv>::new(params);

        let columns: Vec<String> = args.get_multiple(0, "columns")?;
        assert!(columns.is_empty());
        Ok(())
    }

    /// U3 - metadata says `multiple`, the plan says otherwise. Report it; never mis-convert.
    #[test]
    fn get_multiple_on_scalar_slot_is_an_error() {
        let params = ResolvedParameterValues(vec![value_element("columns", "a".into(), 21)]);
        let args = CommandArguments::<TestEnv>::new(params);

        let err = args
            .get_multiple::<String>(0, "columns")
            .expect_err("a scalar slot must not satisfy get_multiple");
        assert!(err.message.contains("columns"), "message: {}", err.message);
    }

    /// U4 - an unresolved link element. Unreachable through the interpreter, reachable through
    /// `CommandArguments::new`, so the message must say what is wrong.
    #[test]
    fn get_multiple_unresolved_link_element_is_an_error() -> Result<(), Error> {
        let link = crate::parse::parse_query("-R/config/colname.txt")?;
        let params = ResolvedParameterValues(vec![ParameterValue::MultipleParameters(vec![
            ParameterValue::ParameterLink("columns".to_string(), link, Position::new(21, 1, 22)),
        ])]);
        let args = CommandArguments::<TestEnv>::new(params);

        let err = args
            .get_multiple::<String>(0, "columns")
            .expect_err("an unresolved link must not convert");
        assert!(err.message.to_lowercase().contains("link"), "message: {}", err.message);
        Ok(())
    }

    /// U5 - the error points at the offending ELEMENT, not at the action. This is what the
    /// per-element positions measured by liquers-validate are for.
    #[test]
    fn get_multiple_conversion_error_carries_element_position() {
        let params = ResolvedParameterValues(vec![ParameterValue::MultipleParameters(vec![
            value_element("rows", 1.into(), 21),
            value_element("rows", "x".into(), 23),
        ])]);
        let mut args = CommandArguments::<TestEnv>::new(params);
        args.action_position = Position::new(6, 1, 7);

        let err = args
            .get_multiple::<i64>(0, "rows")
            .expect_err("\"x\" is not an i64");
        assert_eq!(err.position.offset, 23, "must point at the element, not the action");
    }

    /// U6 - non-string element types, the case that motivates element-type inference.
    #[test]
    fn get_multiple_converts_integers() -> Result<(), Error> {
        let params = ResolvedParameterValues(vec![ParameterValue::MultipleParameters(vec![
            value_element("rows", 1.into(), 21),
            value_element("rows", 2.into(), 23),
        ])]);
        let args = CommandArguments::<TestEnv>::new(params);

        let rows: Vec<i64> = args.get_multiple(0, "rows")?;
        assert_eq!(rows, vec![1, 2]);
        Ok(())
    }
}
```

#### liquers-macro — declaration and rejection (`liquers-macro/src/registration.rs`, existing `mod tests`)

**No `trybuild` dependency is needed**, and this is a deliberate consequence of putting every check
in `impl Parse` rather than in `command_registration()`. A parse-level check is reachable from
`syn::parse2::<CommandSignature>(quote!{…})`, which returns `syn::Result` — so the rejections are
ordinary unit tests that assert on the message text, in the crate that owns them, with no new
dev-dependency and no `tests/ui/*.stderr` files to keep in sync.

The existing tests use `syn::parse_quote!`, which panics on error; the rejection tests use
`syn::parse2` instead.

```rust
/// U7-U9 - a variadic declaration produces the right metadata and the right accessor.
#[test]
fn variadic_parameter_generates_multiple_metadata_and_accessor() {
    let mut sig: CommandSignature = syn::parse_quote! {
        fn select_columns(state, columns: Vec<String> multiple) -> result
    };
    sig.wrapper_version = WrapperVersion::V2;
    let tokens = fuzzy(&sig.command_registration().to_string());

    assert!(tokens.contains("multiple:true"), "tokens: {tokens}");
    assert!(tokens.contains("ArgumentType::String"), "element type, not Any: {tokens}");
    assert!(tokens.contains("arguments.get_multiple(0usize,\"columns\")?"), "tokens: {tokens}");
    assert!(!tokens.contains("arguments.get(0usize,\"columns\")?"), "must not use `get`");
}

/// U10 - the non-variadic path is untouched. Guards the claim that every existing
/// register_command! invocation expands identically.
#[test]
fn scalar_parameter_expansion_is_unchanged() {
    let mut sig: CommandSignature = syn::parse_quote! {
        fn test_fn(state, a: i32) -> result
        label: "Test label"
    };
    sig.wrapper_version = WrapperVersion::V2;
    let tokens = fuzzy(&sig.command_registration().to_string());

    assert!(tokens.contains("multiple:false"));
    assert!(tokens.contains("arguments.get(0usize,\"a\")?"));
}

/// U11-U16 - each malformed declaration is rejected, with a message naming the problem.
#[test]
fn malformed_variadic_declarations_are_rejected() {
    fn err_of(ts: proc_macro2::TokenStream) -> String {
        syn::parse2::<CommandSignature>(ts)
            .err()
            .map(|e| e.to_string())
            .unwrap_or_else(|| panic!("declaration must not parse"))
    }

    // U11 - not a container.
    let m = err_of(quote! { fn f(state, columns: String multiple) -> result });
    assert!(m.contains("container"), "{m}");
    assert!(m.contains("Vec<String>"), "suggests the fix: {m}");

    // U12 - unknown flag. The pre-existing silent-typo hazard.
    let m = err_of(quote! { fn f(state, columns: Vec<String> multipel) -> result });
    assert!(m.contains("multipel"), "{m}");
    assert!(m.contains("injected"), "names the valid flags: {m}");

    // U13 - mutually exclusive.
    let m = err_of(quote! { fn f(state, c: Vec<String> injected multiple) -> result });
    assert!(m.contains("injected") && m.contains("multiple"), "{m}");

    // U14 - duplicate.
    let m = err_of(quote! { fn f(state, c: Vec<String> multiple multiple) -> result });
    assert!(m.to_lowercase().contains("duplicate"), "{m}");

    // U15 - no default on a variadic argument.
    let m = err_of(quote! { fn f(state, c: Vec<String> multiple = "x") -> result });
    assert!(m.contains("default"), "{m}");

    // U16 - VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS, at compile time.
    let m = err_of(quote! { fn f(state, a: Vec<String> multiple, b: i32) -> result });
    assert!(m.contains("b"), "names the starved argument: {m}");
    assert!(m.contains("a"), "names the variadic argument: {m}");
}

/// U17 - the ordering rule's exemptions. An injected argument and `context` consume no query
/// parameter, so neither is starved and both may follow a variadic argument.
#[test]
fn injected_and_context_may_follow_a_variadic_argument() {
    let ok = syn::parse2::<CommandSignature>(quote! {
        fn f(state, a: Vec<String> multiple, p: MyPayload injected, context) -> result
    });
    assert!(ok.is_ok(), "err: {:?}", ok.err().map(|e| e.to_string()));
}
```

`quote` and `syn::parse2` are already available in that test module (`registration.rs:1822`).

### Integration tests

#### liquers-lib — the converted commands (`liquers-lib/tests/polars_commands.rs`)

**These replace the existing `test_select_columns` and `test_drop_columns`**, which do not invoke
the commands at all — they call `polars::DataFrame::select` directly and would keep passing after
this conversion, including if the conversion were wrong. The file's own `create_test_env()` (`:9`)
builds a `DefaultEnvironment` with the polars commands registered and is **never called** by any of
the 13 tests. Filed for the other eleven as `POLARS-COMMAND-TESTS-BYPASS-COMMANDS`; the two this
design touches are fixed here.

```rust
/// Evaluate a query against a CSV state and return the resulting DataFrame.
/// This is the helper the file has been missing: it goes through the command, not around it.
async fn eval_over_csv(csv: &str, query: &str) -> Result<Arc<DataFrame>, Error> {
    let env = create_test_env();            // already defined at :9, previously unused
    let envref = env.to_ref();
    let state = create_csv_state(csv);
    let result = /* apply `query` to `state` via the environment's evaluation entry point */;
    liquers_lib::polars::util::try_to_polars_dataframe(&result)
}

/// I1 - the plain dash spelling, which is an arity error at HEAD, selects two columns.
#[tokio::test(flavor = "multi_thread")]
async fn select_columns_takes_one_parameter_per_column() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a,b,c\n1,2,3\n4,5,6", "ns-pl/select_columns-a-c").await?;
    assert_eq!(df.width(), 2);
    assert!(df.get_column_names().iter().any(|s| *s == "a"));
    assert!(df.get_column_names().iter().any(|s| *s == "c"));
    assert!(!df.get_column_names().iter().any(|s| *s == "b"));
    Ok(())
}

/// I2 - THE new capability. `a~_b` is one escaped parameter, so it names the single column
/// `a-b`. Under the deleted `split('-')` this was indistinguishable from two columns.
#[tokio::test(flavor = "multi_thread")]
async fn select_columns_escaped_dash_names_one_column() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a-b,c\n1,2\n3,4", "ns-pl/select_columns-a~_b").await?;
    assert_eq!(df.width(), 1);
    assert_eq!(df.get_column_names(), vec!["a-b"]);
    Ok(())
}

/// I3 - an empty variadic argument is well-formed at plan level (measured), so the command
/// is what must reject it, and the message must be about columns rather than about arity.
#[tokio::test(flavor = "multi_thread")]
async fn select_columns_with_no_columns_is_rejected() {
    let err = eval_over_csv("a,b\n1,2", "ns-pl/select_columns")
        .await
        .expect_err("selecting no columns is not a meaningful request");
    assert!(err.message.contains("at least one column"), "message: {}", err.message);
}

/// I4 - the second converted command.
#[tokio::test(flavor = "multi_thread")]
async fn drop_columns_takes_one_parameter_per_column() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a,b,c\n1,2,3", "ns-pl/drop_columns-b-c").await?;
    assert_eq!(df.width(), 1);
    assert_eq!(df.get_column_names(), vec!["a"]);
    Ok(())
}

/// I5 - per-element validation still happens: `check_column_exists` runs for each element.
#[tokio::test(flavor = "multi_thread")]
async fn select_columns_reports_the_unknown_column_by_name() {
    let err = eval_over_csv("a,b\n1,2", "ns-pl/select_columns-a-zz")
        .await
        .expect_err("zz does not exist");
    assert!(err.message.contains("zz"), "message: {}", err.message);
}
```

#### Registry (`liquers-lib/tests/registry_export.rs`)

```rust
/// I6 - `multiple: true` together with `argument_type: string` is a serde combination that has
/// never existed in an exported registry (`grep multiple specs/command_registry.yaml` is empty
/// at HEAD). Both fields are independently skipped when at their default, so this confirms the
/// pairing rather than each field.
#[tokio::test]
async fn variadic_argument_round_trips_through_the_registry() -> Result<(), Error> {
    let registry = full_registry()?;
    let key = CommandKey::new("", "pl", "select_columns");
    let before = registry.get(&key).expect("select_columns is registered");

    // `serde_yaml` is a regular dependency of liquers-lib (Cargo.toml:31), and
    // `from_json_or_yaml` takes (name, text) — the same call the file already makes at :136.
    let yaml = serde_yaml::to_string(&registry)?;
    let after: CommandMetadataRegistry = from_json_or_yaml("round-trip", &yaml)?;
    let arg = &after.get(&key).expect("survives the round trip").arguments[0];

    assert!(arg.multiple, "multiple must survive");
    assert_eq!(arg.argument_type, ArgumentType::String, "element type must survive");
    assert_eq!(arg.multiple, before.arguments[0].multiple);
    Ok(())
}
```

`I7` is the existing `registry_export` agreement test, which fails until
`specs/command_registry.yaml` is regenerated. It needs no new code — only the regeneration, and
awareness that the diff will also show changed `impl_version` values, because
`#[command_version]` blake3-hashes each function's whole token stream
(`liquers-macro/src/versioning.rs:15-21`).

## Corner Cases

| # | Case | Expectation | Why it is not obvious |
|---|---|---|---|
| C1 | `ns-pl/select_columns-a-` (trailing dash) | Two elements, the second the empty string; `check_column_exists` rejects `""` by name | `parse.rs` documents `action-` as one *empty* parameter, so an empty element is still an element — the same rule `empty_excess_parameter_is_still_excess` pins for the arity check |
| C2 | A variadic argument in an **aliased** command, where head parameters fill leading slots | The variadic argument consumes what remains after the head parameters | `from_action_extended` skips `n` already-filled slots (`plan.rs:1000`); `accepted_parameter_count` excludes them from the arity message. Untested with `multiple` |
| C3 | A recipe `OverrideValue` on a variadic argument | Overrides flow through unchanged | `from_arginfo` expands an *array* default into `MultipleParameters` (`plan.rs:480`); the macro cannot declare one (D4), but a recipe or hand-built metadata can, so the path stays live |
| C4 | `get_multiple::<Value>` | Does **not compile** — no `impl FromParameterValue<Value> for Value` exists | Deliberate limitation, recorded in Phase 2. `Vec<Value>` remains retrievable for hand-built registrations through the untouched `impl<V: ValueInterface>`; it is only the macro path that cannot express it |

C1 and C2 are worth writing. C3 needs no new test if the existing recipe suite already covers
override flow — Phase 4 checks rather than assumes. C4 is asserted by a comment, not a test, since
a compile failure cannot be asserted without `trybuild`.

## Documentation and Learning Log

### Guide candidates

For `specs/guides/COMMAND_REGISTRATION_GUIDE.md` §"Accepting a variable number of parameters",
which currently says the feature cannot be declared:

| Answers | Material | Executable evidence |
|---|---|---|
| "How do I accept a variable number of parameters?" | The Example 1 declaration and function signature | I1 |
| "How do I pass a value containing the separator?" | The `a-b` vs `a~_b` table from Example 1 | I2 |
| "Why does my `Vec<i32>` argument fail at runtime?" | The Example 2 `ArgumentType` table | U6, U8 |
| "Where does the flag go, and can I combine it with `injected`?" | The grammar line `<name>: <Type> [injected \| multiple] [= default] [(…)]` | U13, U17 |
| "Why won't my declaration compile?" | The Example 3 message table | U11-U16 |

The `~_` escape moves from being the *workaround* the guide currently teaches to being the way to
express one thing rather than two — the same text, an opposite role.

### Learning captured in this phase

Carry these into Phase 5; each changed how the work is planned.

1. **The design was verifiable before it was written.** A registry overlay plus `liquers-validate`
   resolved the proposed signatures and printed the actual plan — three elements with three
   positions, one element for `a~_b`, an empty list for no parameters — with no Rust compiled and
   no store opened. For a change whose runtime half already exists, that is a full check of the
   half that exists. Worth generalising: any design that *changes a signature* can be tested this
   way before Phase 4.
2. **Putting the checks in `impl Parse` removed a dependency.** Because every rejection is a parse
   error, `syn::parse2` reaches it and the messages are asserted by ordinary unit tests. Had the
   checks lived in `command_registration()` instead, they would only surface as `compile_error!`
   at an expansion site and would have required `trybuild`, a new dev-dependency, and `.stderr`
   fixtures to maintain. The architecture choice and the test cost are the same choice.
3. **A test named after a command need not touch it.** All 13 tests in
   `liquers-lib/tests/polars_commands.rs` call Polars directly; `create_test_env()` is defined and
   never called. The two this design touches are fixed here; the rest is
   `POLARS-COMMAND-TESTS-BYPASS-COMMANDS`. The general lesson for Phase 5: when a conversion's
   existing tests would pass unchanged *whatever* the conversion did, that is a finding about the
   tests, not reassurance about the change.
4. **`#[command_version]` hashes the whole function**, so converting a command changes its
   `impl_version` in the exported registry. Expected churn, not a mistake.

## Review Findings Applied

Three passes were run over this document (Phase 1 conformity, Phase 2 conformity, codebase and
query validation), sequentially rather than as parallel agents. Query validation is recorded under
"Validation"; the codebase pass changed three things and confirmed the rest:

| Finding | Resolution |
|---|---|
| The `get_multiple` tests used a placeholder environment alias | `SimpleEnvironment<Value>`, which `commands.rs`'s own test modules already alias (`:733`) |
| `eval_over_csv` left the evaluation call unspecified | Pinned to `interpreter::evaluate` over an `AsyncMemoryStore`, matching the existing integration suites |
| `from_json_or_yaml(&yaml)` — wrong arity | Takes `(name, text)`; corrected to match the call already in `registry_export.rs:136` |
| `registry.get(&key)` | Confirmed valid: `get<K: Into<CommandKey>>` plus `impl From<&CommandKey> for CommandKey` (`command_metadata.rs:591`) |
| `Position::new(offset, 1, offset + 1)` | Confirmed: `Position::new(offset: usize, line: u32, column: usize)` (`query.rs:447`) |
| `err.message`, `err.position.offset` | Confirmed public fields (`error.rs:50-52`) |
| `fuzzy`, `wrapper_version`, `quote`, `syn::parse2` in the macro tests | Confirmed present (`registration.rs:2293`, `:1067`, `:1822`) |
| `serde_yaml` availability in liquers-lib | Confirmed a regular dependency (`Cargo.toml:31`), not just dev |

Phase 1 and Phase 2 conformity passes found no drift: every example traces to a Phase 1 decision
(E1→D3/D4, E2→D2/D3, E3→D1 and the four rejections), and every signature matches Phase 2
(`get_multiple<T: FromParameterValue<T>>`, `variadic_element_type`, the six messages).

## Validation

Every query in this document was checked with `liquers-validate` against
`specs/command_registry.yaml` overlaid with a proposal declaring the two variadic signatures. All
resolve `Ok`; the plan steps are quoted in "Evidence already in hand". `ns-pl/select_columns-a-b-c`
against the unmodified registry reproduces the `TooManyParameters` error, confirming the
before/after.
