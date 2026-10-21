# Phase 2: Solution & Architecture — SQL over a table (`ns-pl/sql`)

## Overview

Enable polars' SQL engine (`polars` feature `sql`, crate `polars-sql` 0.55.2, already in
`Cargo.lock`) and register one synchronous command, `pl/sql`, that reads its input with
`try_to_polars_dataframe`, checks the SQL text against a read-only allow-list, registers the
input as table `self` in a fresh `SQLContext`, executes the statement to a `LazyFrame` and
collects it with `CollectOnAnyRuntime`. The result is a `polars.DataFrame`.

Rejected alternatives:
- **GlueSQL over `RecordView`** — the source issue's candidate. Wasm-capable, but a new dependency
  tree, a row-at-a-time adapter over columnar records, and a second type mapping. Kept as the wasm
  follow-up (`NO-SQL-ENGINE-IN-WASM-BUILDS`), where it is the only option.
- **DataFusion** — the best structural match for `RecordSource` (`record-streams/engine-survey.md`
  §4), but an Arrow-rs stack of its own: hundreds of crates and a second Arrow next to polars'.
- **Joins in this slice** — needs a way to name several tables in one query (links materialized as
  values, keys evaluated by the command, or recipe overrides); a separate contract question, filed
  as `SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES`.
- **An `ns-sql` namespace** — the engine, the input rules and the result are `ns-pl`'s.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `POLARS-BRIDGE-VECTOR-COLUMNS-REFUSED` | draft | P3 | A `RecordView` with a Vector column is refused before SQL runs (AC-3 inherits it) | No | No | Documented as inherited |
| `IPC-READER-CANNOT-READ-ANY-POLARS-STRING-COLUMN` | draft | P2 | None: concerns the records IPC reader, not the polars input path | No | No | None |
| `POLARS-DATAFRAME-IPC-NOT-SERIALIZABLE` | draft | P3 | The result serializes as any `polars.DataFrame` (CSV/Parquet) | No | No | None |

## Interfaces

```rust
// liquers-lib/src/polars/sql.rs  (whole module under the existing `polars` cfg)

/// `ns-pl/sql`: one SQL query statement over the input, registered as table `self`.
#[liquers_macro::command_version]
pub fn sql(state: &State<Value>, query: String) -> Result<Value, Error>;

/// Refuses anything but a single query statement whose relations are `self` or its own CTEs.
/// Returns `Ok(())` or an `Error::general_error` naming the offending statement or relation.
pub(crate) fn check_read_only_sql(query: &str) -> Result<(), Error>;

#[macro_export]
macro_rules! register_polars_sql_commands { ($cr:expr) => {{ /* see Relevant Commands */ }} }
```

Sync, like every `ns-pl` command (`POLARS_COMMAND_LIBRARY.md` §"Async Considerations"): polars
parallelizes internally and `collect_on_any_runtime` already handles current-thread runtimes.
No new struct, trait, `ExtValue` variant or `TypeInfo`.

`check_read_only_sql` parses with `sqlparser::parser::Parser::parse_sql(&GenericDialect {}, q)` —
`sqlparser` becomes a direct dependency, `version = "0.62"`, `features = ["visitor"]`, exactly the
version and features polars-sql already resolves (`sqlparser_derive` is in the lock) — then
requires one `Statement::Query`, collects the CTE aliases of every `With`, and walks
`sqlparser::ast::visit_relations`: a relation whose name is neither `self` nor a CTE alias is
refused. A table function such as `read_csv('…')` is a relation named `read_csv`, so it is refused
by the same rule. Polars' dialect differs slightly from `GenericDialect`; a statement our parse
refuses but polars would accept is a false refusal (safe), never a bypass, because polars parses
the same text again and any relation it would read must have passed the walk.

## Integration Points

- `liquers-lib/Cargo.toml`: `polars` dependency gains feature `"sql"`; new optional dependency
  `sqlparser = { version = "0.62", features = ["visitor"], optional = true }`; feature
  `polars = ["dep:polars", "dep:sqlparser"]` (Q1 recommended form).
- `liquers-lib/src/polars/mod.rs`: `pub mod sql;`, `register_polars_sql_commands!` added to
  `register_polars_commands!`, and `sql::register_commands(cr)?` in `register_commands`.
- `liquers-lib/src/polars/util.rs`: reused unchanged (`try_to_polars_dataframe`,
  `CollectOnAnyRuntime`).
- `specs/command_registry.yaml`: regenerated, with a CHANGELOG line.
- `liquers-web`: none; it forwards `polars` and never enables it for wasm.

## Error Handling

| Failure | Error |
|---|---|
| Empty or whitespace-only `query` | `Error::general_error("sql requires a query …")` |
| Not one query statement; a relation other than `self`/CTE | `Error::general_error` naming the statement kind or relation (AC-5) |
| sqlparser parse failure | `Error::general_error` carrying the parser message |
| polars SQL or execution error (unknown column, type error) | `Error::general_error(format!("SQL failed: {e}"))`, as `ns-pl` maps `PolarsError` today |
| Input conversion (`RecordSource`, unknown format) | unchanged errors of `try_to_polars_dataframe` |
| Terminal input state | propagated by `state.value()?` |

No `unwrap`/`expect`; no panic path.

## Relevant Commands

```rust
register_command!($cr,
    fn sql(state, query: String) -> result
    namespace: "pl"
    label: "SQL"
    doc: "Run one SQL query statement over the input table, named `self` (polars SQL). Only `self` and the statement's own CTEs may be read."
    filename: "data.csv"
    version: auto
)?;
```

`query` is a plain `String`: a link delivering `Value::Text` binds to it
(`COMMAND_REGISTRATION_GUIDE.md` §"Passing the working directory…", "Argument types"), so
`sql-~X~-R/queries/q.sql/-/to_text~E` works with no special parameter kind and records the
dependency (`liquers-core/src/plan.rs`, `DependencyRelation::ParameterLink`). It is not
`multiple`: SQL contains no Liquers separators once escaped, and a self-split would mangle it.
Namespaces involved: `pl` (new command), `rec` (inputs reach it through the bridge).

## Documentation Architecture

- `specs/reference/POLARS_COMMAND_LIBRARY.md` (reference, internal, `lib/polars`): new "SQL"
  subsection under Command Reference — signature, table name `self`, the allow-list, both
  spellings with the `~ncomma~~.` trap, and "Grouping & Aggregation" pointing to it. History row,
  `reviewed:` bump.
- `specs/reference/RECORD_STREAMS.md` (reference): the `ns-rec`/`ns-pl` comparison gains "SQL —
  `ns-pl/sql` only". History row, `reviewed:` bump.
- `affects_docs`: those two. No README link (no new document).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-lib/Cargo.toml`, `liquers-lib/src/polars/{mod.rs,sql.rs}`, `liquers-lib/tests/polars_commands.rs`, `specs/command_registry.yaml`, two references |
| Crates and workflows affected | `liquers-lib` with `polars`; `liquers-py` (enables `polars`, gains the command); `liquers-axum` deployments using `liquers-lib` |
| Existing tests likely to change | `registry_export` (fails until the registry is regenerated); `register_all_commands` counts if they assert a total |
| New validation | Phase 3 tests; `check-build-matrix.sh` rows with and without `polars` |
| Compatibility / data / concurrency / performance / security | Additive. Build cost: `polars-sql` + `sqlparser` compile in every `polars` build (estimate tens of seconds cold, to be measured in Step 1). Security: the allow-list is what keeps SQL from reading the server's filesystem. Performance: one `DataFrame` clone into a `LazyFrame`, as other `ns-pl` commands |
| Recovery | Drop the command registration and the feature line; nothing else references them |
| Certainty and open questions | High on the slice. To re-verify at implementation (sources not in the local registry): the exact path `polars::sql::SQLContext`, `register(&str, LazyFrame)`, `execute(&str) -> PolarsResult<LazyFrame>`, and the table-function names. Q1 (feature placement) is the open choice |
