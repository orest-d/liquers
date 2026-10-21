---
id: STORE-NO-DATABASE-STORAGE-MODEL
kind: feature
title: Metadata records cannot be kept in a database, and write strategies cannot journal a non-object write
status: draft
priority: P3
complexity: L
area: [core/store, store/backends, store/config]
design: store-database-records
created: 2026-10-15
github:
---
## Problem

Every storage model writes records as objects of a store (`per_entry`, `grouped`) or not at all
(`native`, `none`). `design/store-adapters/phase2-architecture.md` §5 planned a two-phase
`PreparedWrite` in `WriteSet` so that a database model (sqlite, sled, fjall) could take part in an
atomic or journalled write. It was not implemented (`phase4-implementation.md`, deviations): the
write strategies handle object writes only, and no database model is registered.

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

## Impact

Large folders pay the cost of rewriting one grouped file per change, and a transactional write
cannot cover a record kept outside the object store.

## Expected behaviour

A `PreparedWrite` (prepare, commit, abort by id) in the write set, journalled by `transactional`,
and at least one database storage model behind a feature.

## Discovery

Out of scope for `design/store-adapters/` (Phase 1 non-goals); interface left unimplemented.

## Re-scoped (2026-10-21)

Designed in `design/store-database-records/` as a SQLite-backed **store** (`sqlite_kv`) used as the
mount of central storage (`design/central-storage-model/`), not as a new storage model. The
`PreparedWrite` half of the original plan is shown unnecessary there: the transactional strategy
already stages, journals and renames across the data store and the record store (`JournalWrite`,
`liquers-core/src/store/writes.rs`). Whether to drop it or keep it as a separate issue is that
design's open question 2.
