---
id: POLARS-COMMAND-TESTS-VIA-QUERIES
kind: design
title: Polars command tests evaluate the commands they are named after
form: compact
status: complete
gh_pr: [170]
readiness: ready
autofix: eligible
area: [lib/polars, build]
issues: [POLARS-COMMAND-TESTS-BYPASS-COMMANDS]
created: 2026-10-19
---
# Polars command tests evaluate the commands they are named after

> **Complete on 2026-10-20** (bulk design review): implemented by #170 (commit f622c95), merged; every source issue is closed and no phase is outstanding.

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written; Phase 4 implemented on
2026-10-20 (automatic fix).

## Phase 1: High-Level Design

### Purpose

Every test in `liquers-lib/tests/polars_commands.rs` that is named after an `ns-pl` command must
evaluate that command through the command registry, so a mis-registered, mis-declared or broken
command fails a test. Every registered `ns-pl` command gets at least one such test.

### Problem Example

At HEAD (`569c2f0`) the issue is half fixed. `create_test_env()` is now used, through the helper
`eval_over_csv` (added by `variadic-arguments-declaration` and `command-alias-contract`), and
`test_select_columns`, `test_drop_columns`, `test_head_alias_matches_slice` and five more go
through `AssetManager::apply`. Ten tests still bypass the commands:
`test_from_csv_basic`, `test_shape`, `test_head`, `test_tail`, `test_slice`, `test_filter_eq`,
`test_sort`, `test_aggregation_sum`, `test_aggregation_mean`, `test_chained_operations`.
`test_sort` is representative:

```rust
let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;
let result = (*df).clone().lazy().sort(["age"], SortMultipleOptions::default()).collect()?;
```

It never names `ns-pl/sort`. Deleting the `sort` registration from
`register_polars_sorting_commands!`, or swapping its `ascending` default, leaves the whole file green.
The `polars` feature alone runs no `ns-pl` filter, sort, aggregation, info or I/O command at all
(`tests/records_table_ops.rs` runs some, but only with `records`).

A second, smaller hole: `eval_over_csv` converts the result with `try_to_polars_dataframe`, which
parses a CSV text value. A command that wrongly returned its input text would still yield a
DataFrame there.

Should be: `test_sort` evaluates `ns-pl/sort-age` over the CSV state and asserts the order of the
`age` column of the returned `polars.DataFrame`; removing the registration fails it.

### Scope and Acceptance Criteria

- **AC-1** Named tests invoke their command
  WHEN any test in `polars_commands.rs` whose name contains an `ns-pl` command name runs
  THEN it evaluates a query naming that command through `create_test_env()` and asserts on the
  returned state
- **AC-2** Every registered command is executed
  WHEN `cargo test -p liquers-lib --no-default-features --features polars --test polars_commands`
  runs
  THEN each of the 32 `pl` commands registered by `register_polars_commands` (31 functions and the
  `head` alias) is evaluated by at least one test
- **AC-3** The result type is checked, not coerced
  WHEN a DataFrame-producing command is evaluated by the helper
  THEN the helper reads the value with `as_polars_dataframe` and fails if the value is not a
  `polars.DataFrame`; the non-DataFrame commands (`shape`, `nrows`, `ncols`, `schema`, `to_csv`)
  assert their own value type
- **AC-4** Arguments and defaults are exercised
  WHEN a command has a default or a non-string argument (`tail` `n = 5`, `sort` `ascending`,
  `starts_with` `case_sensitive`, `from_csv` `separator`)
  THEN one test omits it and one supplies a non-default value, and the results differ as declared
- **AC-5** Malformed input is an error naming the cause
  WHEN `sort`, `eq` or a text filter names a column that does not exist, or a text filter targets a
  numeric column
  THEN evaluation returns `Err` whose message names the column
- **AC-6** Chained queries keep the namespace
  WHEN `ns-pl/eq-city-NYC/select_columns-name-age/sort-age` is evaluated
  THEN all three actions resolve in `pl` and the result has two rows, two columns, sorted by age
- **AC-7** Polars-API tests are not mistaken for command coverage
  WHEN the file is read
  THEN no test calls the Polars API in place of a command; `test_parse_utilities` (which tests
  `polars::util` parsers, not a command) is kept under its name

Non-goals: changing any command; a registry-driven completeness check that fails when a new
command is added without a test (AC-2 pins today's set); unit tests inside `src/polars/`.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — new and corrected integration tests in one crate
  (`liquers-lib/tests/`), no new type, no interface or registry change.
- **Leading issue:** None
- **Explanation:** the helper, the environment and every command already exist; every planned
  query validates against `specs/command_registry.yaml`, and each test asserts a value the command
  source defines.
- **Open questions:** None. Implementation detail, resolved: a converted test that fails on first
  contact is a finding about the command; the implementer files it as an issue and marks that test
  `#[ignore = "<ISSUE-ID>"]` rather than widening this change into a command fix.

### Design Dependencies

- `variadic-arguments-declaration`, `command-alias-contract`, `plan-step-state-metadata` —
  **overlaps** (complete, frozen): they added `eval_over_csv` and the eight query-evaluating tests
  this design keeps unchanged.
- `record-filter-commands` — **overlaps** (complete): `tests/records_table_ops.rs` compares `ns-pl`
  and `ns-rec` under `records`; this design covers the `polars`-only configuration.

### Consolidated Findings

- The issue text is stale on two points: `create_test_env()` is used, and three of its thirteen
  tests (`select_columns`, `drop_columns`, `head` as an alias) already go through queries. Ten
  remain; of the 32 registered `pl` commands, 28 are executed by no test in this file.
- `eval_over_csv` must stop using `try_to_polars_dataframe` (AC-3), or a text-returning command
  passes; existing callers all receive DataFrames, so tightening it breaks nothing.
- `ns-pl` commands use `collect_on_any_runtime`, so the tests keep
  `#[tokio::test(flavor = "multi_thread")]` as the file already does.
- Any first-contact failure becomes its own issue; the test is ignored with that ID.

## Phase 2: Architecture

### Solution

Rewrite the ten bypassing tests in place on top of the existing helpers (they newly execute seven
commands) and add ten tests for the 21 commands still unexecuted after that. One test file changes; no source changes.

Helpers (private, in `polars_commands.rs`):

```rust
/// Apply `ns-pl/<action>` to a CSV text state through the registry; return the raw state.
async fn eval_state_over_csv(csv: &str, action: &str) -> Result<State<Value>, Error>;
/// As above, and require a `polars.DataFrame` value (AC-3).
async fn eval_over_csv(csv: &str, action: &str) -> Result<Arc<DataFrame>, Error>; // body changed
/// Column `name` of `df` rendered with `AnyValue::to_string`, for order and content asserts.
fn column_strings(df: &DataFrame, name: &str) -> Vec<String>;
```

`eval_state_over_csv` is the current body of `eval_over_csv` up to `assetref.get()`;
`eval_over_csv` becomes `eval_state_over_csv(..).await?.value()?.as_polars_dataframe()`.
`test_parse_utilities` stays; `test_head` is renamed `test_head_returns_first_rows` and keeps the
alias test beside it.

Rejected: unit tests calling the command functions directly (`sort(&state, …)`) — they skip
argument parsing, defaults and namespace resolution, which is the gap; a registry-enumerating
completeness test — it changes the contributor workflow for every new command, outside this issue;
moving the tests to `src/polars/` — the registry path needs the integration harness.

Known-issue preflight: `POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME` is closed (the
`collect_on_any_runtime` helper); no open issue blocks. Namespace: `pl` only.

### Changes

`liquers-lib/tests/polars_commands.rs` only: the helpers above; the ten tests rewritten; new tests
listed in Phase 3. No change to `liquers-lib/src/`, the command registry, or documents other than
the issue's status at implementation (closed with a resolution note) and
`specs/reference/POLARS_COMMAND_LIBRARY.md`, which does not describe the test file and needs no
edit.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A command fails on first contact | file an issue, `#[ignore = "<ID>"]`, keep the assertion | medium |
| Float output formatting (`mean`, `std`) differs across polars versions | compare `f64` via `AnyValue::extract::<f64>()` with a tolerance, not strings | high |
| `polars`-only configuration not run by the default loop | Phase 4 runs `--no-default-features --features polars` explicitly | high |
| Test runtime | ~40 small evaluations over a fresh environment each; seconds | high |

Recovery: the change is one test file; revert it.

## Phase 3: Examples and Tests

### Examples

Fixtures (CSV text through `create_csv_state`): `PEOPLE = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,NYC\nDave,28,Chicago"`,
`NUMS = "a,b\n1,2\n3,4\n5,6"`, `WITH_NULL = "name,age\nAlice,30\nBob,\nCarol,35"`,
`SEMI = "a;b\n1;2"`.

| Query (over fixture) | Expected |
|---|---|
| `ns-pl/sort-age` (PEOPLE) | ages `25,28,30,35` |
| `ns-pl/sort-age-f` (PEOPLE) | ages `35,30,28,25` |
| `ns-pl/eq-city-NYC/select_columns-name-age/sort-age` (PEOPLE) | names `Alice,Charlie`, width 2 |
| `ns-pl/shape` (NUMS) | object `{"nrows":3,"ncols":2}` |
| `ns-pl/sum` (NUMS) | `a = 9`, `b = 12` |
| `ns-pl/sort-zz` (PEOPLE) | `Err`, message contains `zz` |

Every query in this phase was checked with `liquers-validate` (two `--query-file` runs, 44
queries): each is `Ok`, its `encoded` form equals its source, and the chained query's three
actions all resolve to `ns: pl`.

### Tests

All in `liquers-lib/tests/polars_commands.rs` (file-level `#![cfg(feature = "polars")]`), each
`#[tokio::test(flavor = "multi_thread")]`:

Rewritten (same names unless stated):
- `test_from_csv_basic` — `from_csv` over PEOPLE: value is a `polars.DataFrame` (`as_polars_dataframe` on the raw state), 4×3: AC-1, AC-3
- `test_from_csv_separator` (new) — `from_csv` vs `from_csv-semicolon` over SEMI: widths 1 and 2: AC-4
- `test_shape` — `shape` (NUMS) via `eval_state_over_csv`, `try_into_json_value` equals the object above: AC-1, AC-3
- `test_head_returns_first_rows` (renamed from `test_head`) — `head-2` over `a\n1..5`: column `a` is `1,2`: AC-1
- `test_tail` — `tail-2` gives `4,5`; `tail` (default 5) gives all five: AC-1, AC-4
- `test_slice` — `slice-1-2` gives `2,3`: AC-1
- `test_filter_eq` — `eq-age-30` (PEOPLE) gives `Alice`: AC-1
- `test_sort` — `sort-age` and `sort-age-f` orders above: AC-1, AC-4
- `test_aggregation_sum` — `sum` (NUMS) `a = 9`, `b = 12`: AC-1
- `test_aggregation_mean` — `mean` (NUMS) `a = 3.0`, `b = 4.0`: AC-1
- `test_chained_operations` — the chained query above: AC-1, AC-6

New:
- `test_info_commands` — `nrows` = 3, `ncols` = 2 (integer values), `schema` object has keys `a`, `b`: AC-2, AC-3
- `test_to_csv_round_trip` — `to_csv` (NUMS) is a text value; parsing it with `from_csv` gives an equal frame: AC-2, AC-3
- `test_comparison_filters` — table over PEOPLE: `ne-age-30` 3 rows, `gt-age-28` 2, `gte-age-30` 2, `lt-age-28` 1, `lte-age-28` 2, `is_in-name-Alice-Bob` 2: AC-2
- `test_null_filters` — WITH_NULL: `is_null-age` gives `Bob`, `is_not_null-age` 2 rows: AC-2
- `test_text_filters` — PEOPLE: `starts_with-name-al` gives `Alice` (case-insensitive default), `starts_with-name-al-t` none, `ends_with-name-E` gives `Alice,Charlie,Dave`, `contains-name-LI` gives `Alice,Charlie`, `contains-name-LI-t` none: AC-2, AC-4
- `test_remaining_aggregations` — NUMS: `median` a = 3, `min` a = 1, `max` a = 5, `std` a = 2.0, `count` a = 3, `describe` has columns `a_mean`, `a_min`, `a_max`, `a_std`: AC-2
- `test_sort_unknown_column_is_rejected` — `sort-zz` error mentions `zz`: AC-5
- `test_eq_unknown_column_is_rejected` — `eq-zz-1` error mentions `zz`: AC-5
- `test_text_filter_on_numeric_column_is_rejected` — `starts_with-age-3` error mentions `age`: AC-5

AC-7 is proved by review of the finished file (no `try_to_polars_dataframe` or `.lazy()` call left
outside the helpers and `test_parse_utilities`) — a grep in Phase 4 step 3. Kept unchanged:
`test_head_alias_matches_slice`, the six `select_columns`/`drop_columns` tests, the two
`slice_*` state-metadata tests, `test_parse_utilities`.

Run: `cargo test -p liquers-lib --no-default-features --features polars --test polars_commands`,
then `cargo test -p liquers-lib --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `liquers-lib/tests/polars_commands.rs` `eval_state_over_csv`, `eval_over_csv`, `column_strings` — split the helper, require a DataFrame in `eval_over_csv` — the eight existing query tests still pass: `cargo test -p liquers-lib --no-default-features --features polars --test polars_commands`. Rollback: revert the file.
- [x] 2. Same file — rewrite the ten bypassing tests and add the ten new ones (Phase 3), one command group at a time; for any failure that is the command's fault, file `specs/issues/<ID>.md` (`status: draft`) and add `#[ignore = "<ID>"]` — same command, with the `--features polars` run green.
- [x] 3. Review — `grep -n "try_to_polars_dataframe\|\.lazy()" liquers-lib/tests/polars_commands.rs` shows only the helpers' and `test_parse_utilities`' lines (AC-7); every registered `pl` command name occurs in a query string (AC-2).
- [x] 4. Records — set `POLARS-COMMAND-TESTS-BYPASS-COMMANDS` to `closed` with a resolution note naming any issues filed in step 2 — `python3 scripts/docs_index.py && python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-lib --no-default-features --features polars --test polars_commands`;
`cargo test -p liquers-lib --lib --tests` (default features); `cargo fmt --check -p liquers-lib`;
`python3 scripts/docs_index.py --check`; a diff review confirming only the test file, the issue
file, any newly filed issues and the generated index changed. No `#[cfg(feature)]` changes, so the
build matrix is not required.
