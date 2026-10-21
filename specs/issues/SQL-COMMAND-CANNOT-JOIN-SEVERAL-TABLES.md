---
id: SQL-COMMAND-CANNOT-JOIN-SEVERAL-TABLES
kind: feature
title: A SQL query cannot read more than one table, so stored entries cannot be joined
status: draft
priority: P3
complexity: M
area: [lib/polars, core/commands]
design:
created: 2026-10-21
github:
---

## Problem

The `ns-pl/sql` command designed in `design/sql-over-tables/` reads exactly one table, its input,
named `self`. A join of two stored entries (`SELECT … FROM self JOIN regions USING (region)`) has
no way to name the second table. The command framework offers three candidate channels and none
fits without a decision:

- a **link argument per table** — a top-level link is materialized as a full value
  (`CommandArguments::get`, `liquers-core/src/commands.rs`), but a `multiple` argument's links are
  converted element-wise through `FromParameterValue`, which cannot carry a DataFrame
  (`convert_multiple_element`), so the number of extra tables would be fixed by the signature;
- **keys evaluated by the command** (`tables: Vec<Key> multiple`, each loaded with
  `context.get_dependency_state`) — works for stored entries, not for derived queries, and the
  table names would have to come from the keys;
- **recipe overrides** naming each table — expressive, but not writable in a plain query.

## Impact

Cross-entry questions still need a hand-written command. Workaround: materialize a joined table by
another command first.

## Expected behaviour

A way to bind several named tables to one SQL statement, with each table a recorded dependency,
chosen among the channels above (or a core change letting a variadic argument carry values).

## Discovery

Split out of `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA` by the 2026-10-21 bulk design
(`design/sql-over-tables/`, Phase 2 rejected alternatives), after reading
`CommandArguments::get_multiple` and `convert_multiple_element` at 18732dd.
