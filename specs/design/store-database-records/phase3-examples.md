# Phase 3: Examples & Use-cases — Metadata records in a database

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Records of a read-only checkout in SQLite | central storage over `sqlite_kv` | AC-3, AC-4 |
| 2 | Example | A SQLite mount as an ordinary store | data and records in one file | AC-1, AC-2 |
| 3 | Test | `c8_sqlite`, `a3_sqlite_per_entry`, `a5_sqlite_central` | conformance | AC-1, AC-2, AC-3 |
| 4 | Test | `sqlite_store::tests::*` | listing bounds, renames, facts, record-only keys | AC-1, AC-2, AC-5 |
| 5 | Test | `store_factory::tests::sqlite_*` | declaration and the feature-off error | AC-6 |

## Example 1: Records of a read-only checkout in SQLite

The Problem Example of Phase 1. After `set_metadata(docs/a.md, title "Plan")` the database holds
one row with key `records/__metadata__/docs/a.md`, `data` the JSON record, and `./docs` is
unchanged; a second `StoreRouterBuilder::from_yaml(…).build_store()` over the same file returns the
title. With `writes: transactional`, a `set` of `docs/b.md` (were `docs` writable) stages
`docs/b.md.__staged__` in the file store and `records/__metadata__/docs/b.md.__staged__` in the
database, writes the journal beside the data, and moves both.

## Example 2: A SQLite mount as an ordinary store

```yaml
stores:
  - type: sqlite_kv
    prefix: cache
    config: { path: ./cache.sqlite, table: objects }
```

`-R/cache/report.csv` is served from a row; `liquers-validate` plans it as
`GetAsset[cache, report.csv]` (validated). Without a `metadata` section the store keeps its own
records in the `metadata` column (native storage).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| key containing `%`, `_` or `'` | listed and read literally | `sqlite_keys_are_literal_in_listings` |
| sibling prefix `a/b` vs `a/bc` | `listdir(a/b)` excludes `a/bc` | `sqlite_listing_is_segment_exact` |
| record-only key | listed, `contains` true, `get` `KeyNotFound` | `sqlite_record_only_key_is_enumerable` |
| `rename_data` onto an existing key | atomic replace | `sqlite_rename_replaces_atomically` |
| bad `table` name (`x; drop`) | configuration error at build | `sqlite_table_name_is_validated` |
| feature off | unavailable, naming `sqlite-store` | `sqlite_type_is_declared_when_compiled_out` |
| 10 000 records in one folder, one rewritten | one row changes (`etag` of others unchanged) | `sqlite_rewrite_touches_one_row` |
| interrupted transactional write | rolled forward on next read | `a5_sqlite_central` (strategy recovery rules) |

## Test Plan

- Unit, `liquers-store/src/sqlite_store.rs` `mod tests` (`:memory:` and a temp file): the edge
  tests above (AC-1, AC-2, AC-5) and `sqlite_data_capabilities_are_all_true`,
  `sqlite_facts_change_on_write` (AC-2).
- Unit, `liquers-store/src/store_factory.rs`: `sqlite_type_is_declared_when_compiled_out` (gated
  `not(feature = "sqlite-store")`, AC-6), `sqlite_type_builds_from_yaml` (AC-1); updated
  `availability02_…` and `default01_…`.
- Conformance, `liquers-store/tests/store_conformance_CONF.rs` (gated on `sqlite-store`):
  `c8_sqlite` (plain suite, `derived_directories: true`, `explicit_directories: false`,
  `stored_metadata: true`, `enumerate_keys: true`; AC-1), `a3_sqlite_per_entry` (`per_entry`
  with `atomic` and with `transactional`; AC-2), `a5_sqlite_central` (central storage of a memory
  member in a `sqlite_kv` mount, with a `per_entry` backup; AC-3, AC-4). Requires
  `central-storage-model`.
- Commands: `cargo test -p liquers-store --features sqlite-store,store-conformance --lib --tests`
  and `bash scripts/check-build-matrix.sh`.

## Learning Log

- The journal's per-target staging is what makes a database a drop-in record store; any future
  record store needs only `list`, an atomic write and an atomic rename.
- Query validated: `-R/cache/report.csv` → `GetAsset[cache, report.csv]`.
