# Phase 1: High-Level Design — Metadata records in a database

## Purpose

Let a deployment keep metadata records (and, optionally, data) in an embedded SQLite database, one
row per key, so a large folder no longer rewrites a whole grouped file per change, record-only keys
cost a row instead of a file, and an all-or-nothing (`transactional`) write covers the records as
it covers files. Serves `VISION.md` priority area 3, "more storage backends — databases and cloud services, through
the store factory model".

## Problem Example

```yaml
metadata: { storage: central-records, writes: transactional }
stores:
  - { type: filesystem, prefix: docs, config: { path: ./docs }, read_only: true }
  - { type: sqlite_kv, prefix: records, config: { path: ./records.sqlite } }
```

Today: no storage model keeps records outside the object stores (`native`, `per_entry`, `grouped`,
`none`; `liquers-core/src/store/storage.rs`), `central-<mount>` refuses to build, and OpenDAL's
`sqlite` service cannot serve as such a mount — it has no `list` (`opendal-0.55.0`
`services/sqlite/backend.rs`, `list: false`), so record-only keys could not be enumerated nor
staged writes recovered. Should be: the store builds; `set_metadata` of `docs/a.md` writes one row
`records/__metadata__/docs/a.md` in `./records.sqlite`, nothing under `./docs`, and a rebuilt
store reads it back.

## Scope and Acceptance Criteria

- **AC-1** A SQLite object store
  WHEN a mount has `type: sqlite_kv` with a `path`
  THEN it is an `AsyncStore` that passes the plain conformance suite with honest capabilities
  (derived directories, stored metadata, `keys()`), creating the file and table when absent
- **AC-2** Data-only access with atomic writes and renames
  WHEN the store is asked for `data_capabilities`
  THEN it reports `data_access`, `atomic_write`, `rename`, `modified_time` and `etag`, and suite
  `A3`-style runs (a `metadata` section with `atomic` and `transactional`) pass over it
- **AC-3** Records in the database
  WHEN the Problem Example's configuration writes and rebuilds
  THEN the record lives in the SQLite file under `records/__metadata__/…` and survives the rebuild
  (with `design/central-storage-model/`)
- **AC-4** All or nothing across stores
  WHEN a `transactional` write of a `docs` key is interrupted after staging (test hook)
  THEN the next read rolls it forward or back, so data and record agree
- **AC-5** Large folders cost per key
  WHEN one record of a folder of 10 000 records is rewritten
  THEN one row is written (no grouped object is read or written)
- **AC-6** Declared when compiled out
  WHEN the `sqlite-store` feature is off and a configuration names `sqlite_kv`
  THEN the build fails naming the feature, as an unavailable (not unknown) store type

Non-goals: `PreparedWrite` and non-object journal entries (Phase 2 shows them unnecessary);
server databases (PostgreSQL, MySQL); querying records with SQL (`NO-SQL-QUERY-CAPABILITY-OVER-…`);
records in a database for a member's own section; migrating sidecars into the database; wasm32.

## Core Interactions

- **Store:** a new `AsyncStore` backend and its `StoreFactory` in `liquers-store`.
- **Store configuration:** used through `central-<mount>`; no new `store_config` entry.
- Query, commands, assets, value types, bindings: untouched.

## Crate Placement

`liquers-store` (new module behind the `sqlite-store` feature): a backend belongs above core, and
this crate already owns the optional backends and their factory (`CLAUDE.md` "Adding a Store
Backend"). Core gains nothing beyond `design/central-storage-model/`.

## Documentation Intent

- Reference: extend `STORE_CONFIG_FSD.md` (the `sqlite_kv` type, its feature) and
  `STORE_SEMANTICS.md` §11 (its `DataCapabilities` row) and §13 (records in a database).
- Guide: extend `STORE_CONFIG_GUIDE.md` §9 (a third complete mount: records in SQLite) and
  `STORE_IMPLEMENTATION_GUIDE.md` §9 (status row of the new store).
- Other documents: none. Documents to update: the four above.

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — a new store type, struct, dependency and feature (rules 3
  and 4); `L`.
- **Leading issue:** **Open design - engine:** which embedded database the store uses.
- **Explanation:** under central storage the strategies already stage, journal and recover across
  stores (`JournalWrite::commit`, `liquers-core/src/store/writes.rs`), so a database needs only to be
  a listable store with atomic writes and renames. The remaining choices are dependency and naming
  decisions; Phases 3-4 assume the recommendations.
- **Open questions:**
  1. **Open design - engine:** `sqlx` with SQLite (recommended: async without `spawn_blocking`,
     already in `Cargo.lock` through OpenDAL's `services-sqlite`, and a likely shared dependency of
     `NO-RELATIONAL-DATABASE-ACCESS-LAYER`), `rusqlite` (sync, every call through `spawn_blocking`),
     or a pure-Rust KV engine such as `redb` (no SQL tooling to inspect the file).
  2. **Open design - re-scope:** close the source's `PreparedWrite` half as not needed
     (recommended: cross-store staging covers the database; a native database transaction would be
     an optimization with no user-visible difference), or keep it as a separate issue.
  3. **Proposed resolution - type name:** `sqlite_kv` (distinct from OpenDAL's `sqlite`, which
     stays as it is).
  4. **Proposed resolution - feature:** `sqlite-store`, off by default, like the database
     services (`liquers-store/Cargo.toml`).

## Design Dependencies

- `central-storage-model` — **requires**: AC-3 and AC-4 use `central-<mount>`; implement it first.
  AC-1, AC-2 and AC-6 do not depend on it.
- `NO-RELATIONAL-DATABASE-ACCESS-LAYER`, `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`
  (issues, another cluster) — **overlaps**, weak: the engine choice (Q1) should agree.
- `store-conditional-write` — **overlaps**, weak: SQLite gives a native atomic `set_if` later.
- `store-adapters` (complete, E4) — origin of the deferred `PreparedWrite`.

## Consolidated Findings

- `PreparedWrite` is unnecessary: staged objects, the journal and `rename_data` already span the
  data store and an `object_store()` (`TargetStores`), and SQLite renames atomically.
- OpenDAL's `sqlite`, `postgresql`, `mysql`, `redis` services cannot be record stores: no `list`.
- The store is ordinary: it must pass the plain suite and an `A3`-style suite before it is used
  for records, so its correctness is checked independently of central storage.
- Prefix listing in SQL needs an exclusive upper bound on the key prefix, not `LIKE` (keys may
  contain `%` and `_`).
