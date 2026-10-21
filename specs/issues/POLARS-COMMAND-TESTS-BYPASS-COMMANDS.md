---
id: POLARS-COMMAND-TESTS-BYPASS-COMMANDS
kind: issue
title: The polars command integration tests never invoke a polars command
status: closed
priority: P2
complexity: M
area: [lib/polars, build]
design: polars-command-tests-via-queries
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

**State at 2026-10-19 (`569c2f0`).** Partly overtaken: `create_test_env()` is now used through
the helper `eval_over_csv`, and `test_select_columns`, `test_drop_columns` and the `head` alias test
go through queries (`variadic-arguments-declaration`, `command-alias-contract`). Ten tests still
bypass the commands — `test_from_csv_basic`, `test_shape`, `test_head`, `test_tail`, `test_slice`,
`test_filter_eq`, `test_sort`, `test_aggregation_sum`, `test_aggregation_mean`,
`test_chained_operations` — and 28 of the 32 registered `pl` commands are executed by no test in the
file. Concrete example: `test_sort` sorts with `LazyFrame::sort` directly, so deleting the `sort`
registration leaves the file green; it should evaluate `ns-pl/sort-age` and assert the returned
order. Design: `specs/design/polars-command-tests-via-queries/`.

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

## Resolution

Fixed 2026-10-20 by `design/polars-command-tests-via-queries/`. Every test in
`liquers-lib/tests/polars_commands.rs` named after an `ns-pl` command now evaluates a query naming
that command through `create_test_env()`, and all 32 registered `pl` commands (31 functions and
the `head` alias) are executed by at least one test. `eval_over_csv` reads its result with
`as_polars_dataframe`, so a command that returned its text input no longer passes as a
DataFrame; the non-DataFrame commands (`shape`, `nrows`, `ncols`, `schema`, `to_csv`) assert their
own value type. Defaults (`tail`, `sort`, the text filters, `from_csv`'s separator) are exercised
both ways, and an unknown column or a text filter on a numeric column is an error naming it. No
command failed on first contact, so no issue was filed. Evidence: `cargo test -p liquers-lib
--no-default-features --features polars --test polars_commands` (30 passed) and the same with the
default features.
