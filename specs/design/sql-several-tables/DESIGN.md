---
id: SQL-SEVERAL-TABLES
kind: design
title: SQL over several named tables — joins in ns-pl/sql
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [lib/polars, core/commands]
issues: [SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES]
created: 2026-10-26
---
# SQL over several named tables — joins in `ns-pl/sql`

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-26: Phases 1-4 written, not
approved, nothing implemented. Follow-up of [`sql-over-tables`](../sql-over-tables/DESIGN.md)
(Phases 1-4 written, not implemented), which this design extends after it lands.

## Phase 1: High-Level Design

### Purpose

Let one SQL statement read several tables, so a question that spans stored entries ("sales per
region *name*", which needs `sales.csv` and `regions.csv`) is a query rather than a hand-written
command. Serves VISION priority area 2 (DataFrames and records) and "useful out of the box".

### Problem Example

`sql-over-tables` gives `ns-pl/sql` one table, its input, named `self`. With
`queries/by_region_name.sql` holding

```sql
SELECT r.name, sum(s.amount) AS total
FROM self AS s JOIN regions AS r ON s.region = r.code
GROUP BY r.name ORDER BY r.name
```

there is no way to say what `regions` is. Today (HEAD cd6d5da) every channel fails:

- a link written as an extra element of a `multiple` argument is materialized as JSON
  (`liquers-core/src/interpreter.rs`, `materialize_link_json` → `try_into_json_value`), so a
  DataFrame cannot pass, and a `-R-key/…` or `…/q` link fails too, because core `Value::Key` /
  `Value::Query` (and `SimpleValue`'s) have no JSON form (`liquers-core/src/value.rs`,
  `try_into_json_value`, falls to `conversion_error`);
- a top-level link is materialized as a full value (`CommandArguments::get`), but only one per
  declared argument, so the number of tables would be fixed by the signature;
- recipe overrides can name values but are not writable in a plain query.

Should be:

```
-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/by_region_name.sql/-/to_text~E-regions-~X~-R/data/regions.csv/-/q~E
```

binds `self` to `data/sales.csv` and `regions` to `data/regions.csv`, returns one row per region
name, and is invalidated when either CSV or the `.sql` file changes. (Validated with
`liquers-validate --command pl/sql`: `Ok`; the inner link plans to `UseQueryValue[data/regions.csv]`.)

### Scope and Acceptance Criteria

- **AC-1** A join over two stored entries
  WHEN the Problem Example is evaluated with `sales.csv` = `region,amount / EU,1 / EU,2 / US,5` and
  `regions.csv` = `code,name / EU,Europe / US,United States`
  THEN the result is a `polars.DataFrame` with rows (Europe, 3) and (United States, 5)
- **AC-2** A derived table
  WHEN a table's link is a derived query (`~X~-R/data/regions.csv/-/ns-pl/eq-active-1/q~E`)
  THEN that query is evaluated and its result is the table, exactly as a stored entry would be
- **AC-3** Each table is a recorded dependency
  WHEN `regions.csv` is overwritten after AC-1 was evaluated
  THEN the cached result is not reused, and the asset's dependency records name the table's query
- **AC-4** Binding errors are reported before SQL runs
  WHEN the table list has an odd number of elements, a name that is not a plain SQL identifier,
  the name `self`, a duplicated name, or an element whose link does not deliver a query
  THEN the asset fails with an error naming the offending element, and no table is evaluated
- **AC-5** The read-only allow-list knows the bound names
  WHEN the SQL reads a relation that is neither `self`, a bound name, nor one of its own CTEs
  THEN it is refused exactly as `sql-over-tables` AC-5 refuses `FROM other`
- **AC-6** Unchanged single-table behaviour
  WHEN `ns-pl/sql` is used with no table list (every `sql-over-tables` example)
  THEN results, errors and the plan are as `sql-over-tables` specifies
- **AC-7** Key and query links inside a variadic argument
  WHEN any command's `multiple` argument contains a `-R-key/…` link or a `…/q` link
  THEN the element arrives as the key's or query's encoded text (so `Vec<String>` and `Vec<Key>`
  arguments accept it) instead of failing with a JSON conversion error

Non-goals: more than one input-table *value* channel (a variadic argument carrying DataFrames —
rejected in Phase 2); joins in `ns-rec` or in wasm (`NO-SQL-ENGINE-IN-WASM-BUILDS`); recipe
`links:` naming tables (works already, through the same argument, once it is a list); SQL writes.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — changes a command signature, so
  `specs/command_registry.yaml` changes (auto-fix rule 4), and spans `liquers-core` and
  `liquers-lib` (rule 6).
- **Leading issue:** **Open design - table naming:** how a query names each extra table.
- **Explanation:** the mechanism is settled by repository evidence — tables travel as query values
  and the command evaluates them through `Context::get_dependency_state`, which records each as a
  dependency; only the spelling of names is a user-facing choice, and every option below yields a
  working design. Phases 3-4 assume the recommended answers.
- **Open questions:**
  1. **Open design - table naming:** (a) explicit pairs `name-link` in one `multiple` argument
     (`…-regions-~X~…/q~E`), **recommended**: readable SQL, the binding is in the query, no
     derivation rule; (b) positional names `t1`, `t2`, … (shorter query, SQL that only reads
     correctly beside its query); (c) names derived from each link's key (`regions` from
     `data/regions.csv`; two derived tables of one key collide, a keyless query has no name).
  2. **Proposed resolution - one command, not two:** extend `ns-pl/sql` with a trailing
     `tables: Vec<String> multiple` argument (the existing spelling stays valid, AC-6) rather than
     adding `ns-pl/sql_join`. The command becomes `async` with `context`; that is invisible in a
     query.
  3. **Proposed resolution - variadic key/query links (AC-7):** fixed in `materialize_link_json`
     only (encoded text when the value has no JSON form but is a key or query), not by giving
     `Value::Key`/`Value::Query` a JSON form everywhere, so `as_bytes("json")` and the type
     registry's write formats are unchanged.

### Design Dependencies

- `sql-over-tables` — **requires**: the `ns-pl/sql` command, `check_read_only_sql`, the polars
  `sql` feature. This design changes that command's signature, so it lands after it (or, if both
  are implemented together, `sql-over-tables` Step 3 registers the extended signature directly).
- `variadic-link-array-expansion` — **overlaps** (weak): it changes
  `materialize_nested_parameter` in the same file; this design changes `materialize_link_json`.
  Either order; an array-valued link is not a table binding.
- `relational-database-access`, `external-index-reconciliation` — **overlaps** (weak): other SQL
  surfaces, no shared contract.

### Consolidated Findings

- **Mechanism:** the only channel that carries arbitrary tables without a core redesign is a query
  value evaluated by the command (`Context::get_dependency_state` records the dependency; the
  `/q` link itself is a `UseQueryValue` step and carries no data dependency).
- **Core defect found and fixed in scope (AC-7):** key and query links inside any `multiple`
  argument fail today, because neither value has a JSON form. Fixing it is one function.
- **Spelling:** a table is `<name>-~X~<query>/q~E`; forgetting `/q` gives a DataFrame link, which
  fails JSON conversion with a message that must name `/q` (AC-4).
- **Security:** the allow-list of `sql-over-tables` stays the guard; bound names are added to it,
  and tables are read only through Liquers queries, never by SQL table functions.

## Phase 2: Architecture

### Solution

`ns-pl/sql` gains a trailing variadic argument whose elements alternate *name*, *query*. The
query elements are links ending in `/q`, so they arrive as query values, materialized to their
encoded text. The command, now `async`, parses each, evaluates it with
`context.get_dependency_state` (which records the dependency), converts the result with
`try_to_polars_dataframe`, and registers it under its name in the same `SQLContext` as `self`.
`check_read_only_sql` accepts the bound names.

Rejected: a variadic argument carrying values (materialize link elements as `E::Value` beside the
parameters) — a change to `CommandArguments`, the interpreter and the macro for one command;
`tables: Vec<Key>` — stored or recipe-keyed entries only, no derived queries; recipe overrides
only — not writable in a query; a fixed number of optional table links — `Option<T>` is limited to
scalars by `register_command!`.

### Changes

- `liquers-core/src/interpreter.rs` `materialize_link_json`: when `value.try_into_json_value()`
  fails, try `value.try_into_query()` then `value.try_into_key()` (both `ValueInterface`
  methods) and return the encoded text as `serde_json::Value::String`; if both fail, return the
  original JSON error. No signature changes.
- `liquers-lib/src/polars/sql.rs`:

  ```rust
  /// `ns-pl/sql`: one SQL query statement over the input (`self`) and the named tables.
  #[liquers_macro::command_version]
  pub async fn sql<E: Environment<Value = Value>>(
      state: State<Value>,
      query: String,
      tables: Vec<String>,
      context: Context<E>,
  ) -> Result<Value, Error>;

  /// Pairs `[name, query, name, query, …]` → `(name, Query)`; refuses an odd count, a non
  /// identifier, `self`, a duplicate, or a text that does not parse as a query.
  pub(crate) fn table_bindings(tables: &[String]) -> Result<Vec<(String, Query)>, Error>;

  /// As in `sql-over-tables`, plus `bound`: names accepted as relations.
  pub(crate) fn check_read_only_sql(query: &str, bound: &[&str]) -> Result<(), Error>;
  ```

  Order inside `sql`: trim/empty check → `table_bindings` → `check_read_only_sql` → input frame →
  `context.submit(&q)` for every binding, then `context.wait_for_dependency` for each, `try_to_polars_dataframe` →
  `SQLContext::register` each → `execute` → `collect_on_any_runtime`. Submitting all before
  waiting (`COMMAND_REGISTRATION_GUIDE.md` §"Waiting for dependencies") lets the tables evaluate
  concurrently. Generic over `E` as the `ns-rec` commands with a context are
  (`liquers-lib/src/records/commands.rs`); the registration macro's `CommandEnvironment` fixes it.
- Registration (replaces `sql-over-tables`' `fn sql(state, query: String)`):

  ```rust
  register_command!($cr,
      async fn sql(state, query: String, tables: Vec<String> multiple, context) -> result
      namespace: "pl"
      label: "SQL"
      doc: "Run one SQL query statement over the input table (`self`) and the named tables given as name, query pairs (`regions-~X~-R/data/regions.csv/-/q~E`)."
      filename: "data.csv"
      version: auto
  )?;
  ```

- Errors: `Error::general_error` naming the element index and text for every AC-4 case; an element
  that parses neither as a query (a DataFrame link without `/q` reaches the command as a failed
  JSON conversion first) gets a message saying "end the table link with `/q`"; dependency
  evaluation errors propagate unchanged.
- Documents: `specs/reference/POLARS_COMMAND_LIBRARY.md` (`sql` section: tables, `/q`, allow-list);
  `specs/reference/COMMAND_DECLARATION.md` or `REGISTER_COMMAND_FSD.md` only if it states what a
  link inside `multiple` delivers (check at Step 1); `specs/command_registry.yaml` (generated).

### Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/interpreter.rs`; `liquers-lib/src/polars/{sql.rs,mod.rs}`; `liquers-lib/tests/polars_commands.rs`; `specs/command_registry.yaml`; one or two references |
| Existing tests likely to change | `registry_export` (regenerate); `sql-over-tables`' tests unchanged in behaviour (AC-6) |
| Compatibility | Additive for queries. AC-7 turns a failure into text for key/query links in variadic arguments — no command relied on the failure (checked: no test asserts it) |
| Concurrency / performance | Each table is a separate asset, evaluated concurrently and cached; one `DataFrame` clone per table into `LazyFrame` |
| Security | Unchanged guard: tables only through Liquers queries; the allow-list now has more names, each bound to an evaluated value |
| Recovery | Revert the registration to `sql-over-tables`' form; the core change stands alone |
| Certainty | High on mechanism (`get_dependency_state`, `q` → `UseQueryValue` verified at HEAD); medium on how `ExtValue`'s `try_into_query` forwards for `CombinedValue::Base` — re-read at Step 1 |

## Phase 3: Examples and Tests

### Examples

1. Join (AC-1): the Problem Example, `/sales_by_region_name.csv` appended. Plan:
   `GetAsset[data/sales.csv]` → `sql(query = <link text>, tables = ["regions", <link>])`.
2. Derived table (AC-2):
   `-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/by_region_name.sql/-/to_text~E-regions-~X~-R/data/regions.csv/-/ns-pl/eq-active-1/q~E`
   (validated: `Ok`).
3. Single table, unchanged (AC-6): `-R/data/sales.csv/-/ns-pl/sql-~X~-R/queries/q.sql/-/to_text~E`.

| Case | Expected | Scenario |
|---|---|---|
| `…-regions~E` missing query (odd count) | error "table list must be name, query pairs" | AC-4 |
| name `self`, `2x`, duplicated `regions` | error naming the element | AC-4 |
| `…-regions-~X~-R/data/regions.csv~E` (no `/q`) | error mentioning `/q` | AC-4 |
| SQL reads `FROM other` with only `regions` bound | refused, names `other` | AC-5 |

### Tests

In `liquers-lib/tests/polars_commands.rs` (file gated on `polars`), with the store-backed helper
`sql-over-tables` adds (memory store holding `data/sales.csv`, `data/regions.csv`,
`queries/by_region_name.sql`):

- `test_sql_joins_two_stored_tables` (AC-1)
- `test_sql_joins_a_derived_table` (AC-2)
- `test_sql_table_change_invalidates` (AC-3): overwrite `data/regions.csv` (rename `Europe` →
  `EU-27`), re-evaluate, assert the new name; assert the dependency records of the first result
  contain the `regions.csv` query
- `test_sql_bad_table_bindings_are_refused` (AC-4): the four error rows above
- `test_sql_single_table_unchanged` (AC-6): Example 3 equals the `sql-over-tables` result
- Unit, `liquers-lib/src/polars/sql.rs`: `table_bindings_pairs_and_refusals` (AC-4),
  `read_only_check_accepts_bound_names` and `read_only_check_refuses_unbound_name` (AC-5)
- Unit, `liquers-core/src/interpreter.rs` `mod tests`: `variadic_key_link_arrives_as_text` and
  `variadic_query_link_arrives_as_text` (AC-7) — a `multiple` `Vec<String>` test command fed
  `~X~-R-key/a/b~E` and `~X~-R/a/b/-/q~E`; expects `"a/b"` and the encoded query
- Commands: `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test polars_commands sql`,
  `cargo test -p liquers-lib --lib polars::sql`, `cargo test -p liquers-core --lib interpreter`

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/src/interpreter.rs` `materialize_link_json` — key/query fallback; the two
  AC-7 unit tests — `cargo test -p liquers-core --lib interpreter`. Re-check whether a reference
  states what a link inside `multiple` delivers and update it. Depends on nothing.
- [ ] 2. `liquers-lib/src/polars/sql.rs` `table_bindings`, `check_read_only_sql(query, bound)` and
  their unit tests — `cargo test -p liquers-lib --lib polars::sql`. Depends on `sql-over-tables`
  being implemented.
- [ ] 3. `sql` becomes async with `tables` and `context`; registration as Phase 2 —
  `cargo check -p liquers-lib` and `cargo check -p liquers-lib --no-default-features --features records`.
  Depends on 1-2.
- [ ] 4. Integration tests of Phase 3 in `polars_commands.rs`; validate each query with
  `liquers-validate` first — `CARGO_INCREMENTAL=0 cargo test -p liquers-lib --test polars_commands sql`.
- [ ] 5. Regenerate `specs/command_registry.yaml` with a CHANGELOG line ("`pl/sql`: `tables`
  argument") — `cargo test -p liquers-lib --test registry_export`.
- [ ] 6. `POLARS_COMMAND_LIBRARY.md` `sql` section (History row, `reviewed:`); close
  `SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES` with a Resolution; this design's status;
  `python3 scripts/docs_index.py` then `--check`.

### Validation

`cargo test -p liquers-lib --lib --tests`; `bash scripts/check-build-matrix.sh` (a `polars`-gated
command changed shape); diff review: no `println!`, no `unwrap`/`expect` outside tests, explicit
match arms. Steps 3 and 5 land in one PR (`registry_export` fails between them).
