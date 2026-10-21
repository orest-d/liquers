# Phase 4: Implementation Plan — SQL over a table (`ns-pl/sql`)

## Overview

Feature line first (so the build cost is measured before any code depends on it), then the
read-only check with its unit tests, then the command and its registration, then the integration
tests, the generated registry and the documentation. Assumes Q1's recommended answer (enable
`polars/sql` from `polars`); the alternative changes only Step 1 and the feature-matrix rows.
No prerequisite issue.

## Progress

- [ ] Step 1: enable polars SQL and add `sqlparser` under the `polars` feature
- [ ] Step 2: `check_read_only_sql` and its unit tests
- [ ] Step 3: the `sql` command and its registration
- [ ] Step 4: integration tests in `polars_commands.rs`
- [ ] Step 5: regenerate `specs/command_registry.yaml`
- [ ] Step 6: documentation, issue and index maintenance
- [ ] Step 7: feature matrix and final diff review

## Implementation Steps

### Step 1: enable polars SQL and add `sqlparser` under the `polars` feature
- Files / symbols: `liquers-lib/Cargo.toml` `[dependencies] polars`, `[features] polars`
- Change: add `"sql"` to polars' feature list; add
  `sqlparser = { version = "0.62", features = ["visitor"], optional = true }`; set
  `polars = ["dep:polars", "dep:sqlparser"]`. `Cargo.lock` must not gain a package (both are
  already resolved); if it does, stop and re-check versions.
- Depends on: none
- Proof: `CARGO_INCREMENTAL=0 cargo check -p liquers-lib`; `git diff Cargo.lock` shows no new
  package; record the cold-build delta in the PR description
- Rollback: revert the two lines

### Step 2: `check_read_only_sql` and its unit tests
- Files / symbols: new `liquers-lib/src/polars/sql.rs` `check_read_only_sql`, `mod tests`
- Change: parse with `Parser::parse_sql(&GenericDialect {}, query)`; require exactly one
  `Statement::Query`; gather CTE aliases from every `Query::with`; walk
  `sqlparser::ast::visit_relations`, refusing any name (last identifier, case-insensitive) that is
  not `self` or a CTE alias. Errors via `Error::general_error`. Re-open sqlparser 0.62's
  `visit_relations` signature and `Statement`/`With` shapes before writing; the match on
  `Statement` uses an explicit `Statement::Query(_)` arm and a single refusing arm that names the
  statement (sqlparser's enum is foreign and very large; the CLAUDE.md no-`_ =>` rule targets
  Liquers enums).
- Depends on: Step 1
- Proof: `cargo test -p liquers-lib --lib polars::sql` — the six `read_only_check_*` tests (AC-5)
- Rollback: delete the module (not yet registered)

### Step 3: the `sql` command and its registration
- Files / symbols: `liquers-lib/src/polars/sql.rs` `sql`, `register_polars_sql_commands!`,
  `register_commands`; `liquers-lib/src/polars/mod.rs` `pub mod sql`, `register_polars_commands!`,
  `register_commands`
- Change: `sql` trims and rejects an empty query; calls `check_read_only_sql`; reads the input
  with `try_to_polars_dataframe`; `SQLContext::new()`, `register("self", (*df).clone().lazy())`,
  `execute(&query)`, `collect_on_any_runtime()`; maps `PolarsError` to
  `Error::general_error(format!("SQL failed: {e}"))`; returns `Value::from_polars_dataframe`.
  Registration as Phase 2 §Relevant Commands, `version: auto`. Re-open `polars::sql` at 0.55.2
  for the exact `SQLContext` path and signatures.
- Depends on: Step 2
- Proof: `cargo check -p liquers-lib` and
  `cargo check -p liquers-lib --no-default-features --features records` (module compiled out)
- Rollback: remove the macro call and the `pub mod` line

### Step 4: integration tests in `polars_commands.rs`
- Files / symbols: `liquers-lib/tests/polars_commands.rs` — the seven tests of Phase 3 and a
  store-backed environment helper
- Change: as Phase 3 §Test Plan; validate each query string with `liquers-validate` first
- Depends on: Step 3
- Proof: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test polars_commands sql` (AC-1…AC-6)
- Rollback: delete the tests

### Step 5: regenerate `specs/command_registry.yaml`
- Files / symbols: `specs/command_registry.yaml` (generated; CHANGELOG block by hand)
- Change: `cargo run -p liquers-lib --features cli --bin export-command-registry -- --format yaml -o specs/command_registry.yaml`;
  dated CHANGELOG line "added `pl/sql`"
- Depends on: Step 3
- Proof: `cargo test -p liquers-lib --test registry_export`; `liquers-validate -- '<Example 1>'`
  without `--command` now plans
- Rollback: restore the file

### Step 6: documentation, issue and index maintenance
- Files: `specs/reference/POLARS_COMMAND_LIBRARY.md`, `specs/reference/RECORD_STREAMS.md` (Phase 2
  §Documentation Architecture, each with a History row and `reviewed:` bump);
  `specs/issues/NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA.md` → `closed` with a
  Resolution naming the split-off issues; this design's `DESIGN.md` status; `specs/index.csv`
  via `python3 scripts/docs_index.py`
- Depends on: Steps 4-5
- Proof: `python3 scripts/docs_index.py --check`
- Rollback: revert the documents

### Step 7: feature matrix and final diff review
- Change: none; checks only
- Depends on: Steps 1-6
- Proof: `bash scripts/check-build-matrix.sh` (a feature's dependency list changed);
  `cargo test -p liquers-lib --lib --tests`; diff review for scope, no `println!`, no
  `unwrap`/`expect` outside tests, no stray files
- Rollback: n/a

## Testing Plan

Unit tests at Step 2, integration at Step 4, registry freshness at Step 5, then the default loop
and the build matrix at Step 7. The `records`-dependent tests carry `#[cfg(feature = "records")]`.

## Rollback Plan

Every step is additive. Reverting Steps 1-5 removes the command entirely; a partial landing
without Step 5 fails `registry_export`, so Steps 3 and 5 land in one PR.

## Documentation Updates

`POLARS_COMMAND_LIBRARY.md` and `RECORD_STREAMS.md` (History rows, `reviewed:`), the generated
registry with its CHANGELOG line, `specs/index.csv`. The design moves to `complete` after
implementation under `DOCS_STRUCTURE_GUIDE.md` §5.

## Phase 5 Entry Criteria

None — four-phase design.
