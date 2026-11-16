---
id: RECORD-ACCESSOR-STEPS-HAVE-NO-COMMANDS
kind: feature
title: Record rows, columns and cells cannot be projected from a query
status: draft
priority: P3
complexity: S
area: [lib/value, records]
design:
created: 2026-11-16
github:
---
## Problem

`settable-value-accessor` added `RowStep`, `ColumnStep` and `CellStep`
(`liquers-lib/src/records/accessor.rs`): projections of an accessor whose value is a record view to
one row, one column or one cell. They are Rust API only. No `acc` command builds them, so a query
cannot name a table cell (`-R-key/data/table.csv/-/ns-rec/to_record/ns-acc/…/cell-2-n` does not
exist), and neither the web API (`/api/acc`), Python's `Accessor.project` nor JavaScript's can reach
them, since all three project by appending query actions.

## Expected

Projection commands in namespace `acc` (or `rec`), registered with the records feature:
`row-<i>`, `column-<name>` and `cell-<row>-<column>`, each taking an accessor as state and returning
its projection, with the invalidation sink the other projection commands set. A table read from a
stored file must also be writable back in its format, which needs the accessor of the file's value
view (`store`) to accept a record view.

## Notes

- The steps exist and are tested (`record_row_column_and_cell_projections` in
  `liquers-lib/tests/value_accessor_commands.rs`).
- A row's or column's anchor should change when rows are inserted before it, as `ItemStep`'s does;
  check before exposing them to concurrent editors.
