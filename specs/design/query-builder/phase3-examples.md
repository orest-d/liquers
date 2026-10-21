# Phase 3: Examples & Use-cases — query-builder

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Build a resource-then-actions query | builder, headers, escaping | AC-1, AC-3 |
| 2 | Example | `liquers-encode` from a shell | CLI, round trip through the validator | AC-5 |
| 3 | Test | `query_builder::tests::built_queries_round_trip` | corpus of awkward values | AC-1 |
| 4 | Test | `query_builder::tests::invalid_tokens_are_parameter_errors` | each token class | AC-2 |
| 5 | Test | `query_builder::tests::actions_after_resource_get_a_header` | structure | AC-3 |
| 6 | Test | `query::tests::check_encodable_agrees_with_round_trip` | verifier vs. real parse | AC-4 |
| 7 | Test | `liquers-core/tests/liquers_encode_cli.rs` | CLI output, exit codes | AC-5, AC-6 |
| 8 | Test | `escape::tests::token_predicates_match_the_parser` and existing `liquers-template` placeholder tests | shared predicates | AC-7 |
| 9 | Test | `liquers-web` `web_encode_param_accepts_the_empty_string` and `query_builder::tests::empty_parameter_round_trips` | empty value | AC-8 |

## Example 1: build a resource-then-actions query

```rust
use liquers_core::query_builder::QueryBuilder;

let q = QueryBuilder::new()
    .resource(["data", "report.csv"])?
    .ns("pl")?
    .action("head", ["5"])?
    .filename("data.csv")?
    .build();
assert_eq!(q.encode(), "-R/data/report.csv/-/ns-pl/head-5/data.csv");
```

Validated: `liquers-validate -- '-R/data/report.csv/-/ns-pl/head-5/data.csv'` →
`GetAsset[data/report.csv]`, `action pl/slice(n=5)` (alias of `head`), `Filename[data.csv]`.

Errors: `.resource(["data", "../etc"])` → `"../etc" is not valid as a key segment`;
`.action("to text", [] as [&str; 0])` → `"to text" is not valid as an action name`;
`.filename("report")` → not a filename (no extension).

## Example 2: `liquers-encode` from a shell

```bash
$ liquers-encode --resource data --resource report.csv \
    --action filter --arg 12:30 --arg=-5 --arg 'a b' --action to_text
-R/data/report.csv/-/filter-12~ncolon~30-~5-a~.b/to_text
$ liquers-encode --resource 'my file.csv'
error: "my file.csv" is not valid as a key segment: …        (exit 1, stdout empty)
$ liquers-encode --arg x
error: --arg must follow an --action                            (exit 2)
```

The first output, given to `liquers-validate --command filter --`, plans as `GetAsset[data/report.csv]`,
`action filter(12:30, -5, a b)`, `action to_text()`, with `encoded` equal to the input.

## Edge and Error Cases

| Case | Expected | Test |
|---|---|---|
| parameter values `""`, `-`, `~`, `~X~`, `/`, `-5`, `12:30`, `café`, `😀`, `a b` | round-trip exactly | 3, 9 |
| link parameter whose inner query has an invalid token | `ParameterError` from `action_request` | 4 |
| `selector` before any `resource` | `ParameterError` | 4 |
| `action` after `filename` | `ParameterError` | 4 |
| `resource_key(&parse_key("./data")?)` | accepted; encodes `-R/./data` | 6 |
| a hand-built query: transform after resource with no header | `check_encodable` errs | 6 |
| `Łoad` as an action name | refused by the builder; refused by the parser once the predicates are shared | 4, 8 |

## Test Plan

- Unit: `liquers-core/src/query_builder.rs` `mod tests`:
  - `built_queries_round_trip` (AC-1): for each value in the edge table, as the single parameter
    of `filter` after `resource(["data","x.csv"])`, assert `parse_query(&q.encode())? == q` and the
    parameter's `string_value()` equals the value.
  - `invalid_tokens_are_parameter_errors` (AC-2): one invalid example per token class and per
    structure rule; assert `ErrorType::ParameterError` and that the message contains the value.
  - `actions_after_resource_get_a_header` (AC-3): the encoding contains `/-/`; the parsed query has
    two segments.
  - `empty_parameter_round_trips` (AC-8).
- Unit: `liquers-core/src/query.rs` `check_encodable_agrees_with_round_trip` (AC-4): over the
  builder corpus plus hand-built bad queries, `check_encodable().is_ok()` equals
  `parse_query(&q.encode()).is_ok_and(|p| p == q)`.
- Unit: `liquers-core/src/escape/tests.rs` `token_predicates_match_the_parser` (AC-7): for
  sample strings including `Łoad`, `is_identifier(s)` equals "`parse_query(s)` is a single action
  named `s`".
- Integration: `liquers-core/tests/liquers_encode_cli.rs`, `#![cfg(feature = "cli")]`, running
  `env!("CARGO_BIN_EXE_liquers-encode")` (AC-5, AC-6): Example 2's three invocations; stdout
  parses back with `parse_query` to the expected values.
- Existing: `liquers-template` placeholder tests pass unchanged (AC-7).
- `liquers-web`: `web_encode_param_accepts_the_empty_string` replaces the refusal test (AC-8).
- Commands: `cargo test -p liquers-core --lib query_builder query escape`,
  `cargo test -p liquers-core --features cli --test liquers_encode_cli`,
  `cargo test -p liquers-template`.

## Learning Log

- Without a header on the transform segment, `Query::encode` writes `-R/a/b/act`, which the
  parser reads as a key. The builder and `check_encodable` own this rule. The query model does not
  enforce it.
