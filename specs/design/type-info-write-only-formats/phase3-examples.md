# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| T1 | unit (`type_system.rs`) | `with_write_only_data_format("html")` → `supports_data_format("html")` true, `can_read_data_format("html")` false, `can_read_data_format("csv")` true when csv declared |
| T2 | unit | serde: empty list omitted; JSON without the field deserializes |
| T3 | unit (`assets.rs`) | A stored entry whose type declares its format write-only: `try_fast_track` → `Ok(false)` without deserializing (use a test `TypeInfo` registered in a test registry) |
| T4 | integration (`liquers-lib/tests/record_typeinfo.rs`) | `RecordView` declares `html` write-only |
| T5 | integration | A keyed recipe producing a `RecordView` stored as `table.html`: dropping the live asset and requesting again recomputes (command counter), with no error |

T5 recipe query: `ns-rec/…` command producing a view, with filename `table.html` in the recipe,
validated with `liquers-validate`.
