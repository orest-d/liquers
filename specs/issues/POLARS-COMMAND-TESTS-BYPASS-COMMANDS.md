---
id: POLARS-COMMAND-TESTS-BYPASS-COMMANDS
kind: issue
title: The polars command integration tests never invoke a polars command
status: draft
priority: P2
complexity: M
area: [lib/polars, build]
design:
created: 2026-08-25
github:
---
## Problem

`liquers-lib/tests/polars_commands.rs` contains 13 `#[tokio::test]` functions named after polars
commands — `test_select_columns`, `test_drop_columns`, `test_filter_eq`, `test_from_csv_basic` and
so on. **None of them calls a command.** All 13 call
`liquers_lib::polars::util::try_to_polars_dataframe` and then operate on the resulting DataFrame
with the Polars API directly.

`test_select_columns` (`:105`) is representative:

```rust
let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

// Test column selection
let result_df = df.select(["a", "c"])?;
assert_eq!(result_df.width(), 2);
```

`select_columns` — the Liquers command — is never mentioned. The test asserts that
`polars::DataFrame::select` selects columns.

The file even builds the machinery to do it properly and then does not use it: `create_test_env()`
(`:9`) constructs a `DefaultEnvironment`, calls `register_polars_commands()`, and installs a recipe
provider. It is **defined and never called** — its only occurrence in the file is its own
definition.

## Impact

The whole file passes regardless of what the polars commands do. Nothing in it would fail if a
command were mis-registered, given the wrong argument type, wrongly namespaced, or deleted from the
registration macro entirely. Specifically unverified:

- argument declaration and parsing (the `register_command!` metadata),
- the command function's own argument handling and error paths,
- namespace resolution (`ns-pl/`),
- the command's behaviour on malformed input, which is where the interesting failures are.

`cargo test -p liquers-lib --tests` reports 13 passing polars tests, which is what makes this
costly: the coverage is believed to exist.

## Expected behaviour

A test named after a command evaluates a query that invokes it — through
`create_test_env()`, which already exists for exactly this purpose — and asserts on the resulting
`State`. Tests of the Polars API itself, if wanted at all, should be named so they are not mistaken
for command coverage.

## Fix direction

`create_test_env()` is the whole starting point; the file needs a second helper that evaluates a
query string against it and returns the resulting state. Then each test becomes "evaluate
`ns-pl/<command>-<args>` over a CSV state, assert on the DataFrame that comes back".

Do this per command rather than in one sweep — several will fail on first contact, and each failure
is a genuine finding about a command that has never been executed by a test.

## Discovery

Found while planning tests for `specs/design/variadic-arguments-declaration/`, which converts
`select_columns` and `drop_columns` to variadic arguments. Their existing tests would keep passing
unchanged after the conversion, including if the conversion were wrong — which is what surfaced the
problem. That design replaces those two tests with query-evaluating ones; the remaining eleven are
this issue.
