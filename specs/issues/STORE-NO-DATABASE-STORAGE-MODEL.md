---
id: STORE-NO-DATABASE-STORAGE-MODEL
kind: feature
title: Metadata records cannot be kept in a database, and write strategies cannot journal a non-object write
status: draft
priority: P3
complexity: L
area: [core/store, store/backends, store/config]
design:
created: 2026-10-15
github:
---
## Problem

Every storage model writes records as objects of a store (`per_entry`, `grouped`) or not at all
(`native`, `none`). `design/store-adapters/phase2-architecture.md` §5 planned a two-phase
`PreparedWrite` in `WriteSet` so that a database model (sqlite, sled, fjall) could take part in an
atomic or journalled write. It was not implemented (`phase4-implementation.md`, deviations): the
write strategies handle object writes only, and no database model is registered.

## Impact

Large folders pay the cost of rewriting one grouped file per change, and a transactional write
cannot cover a record kept outside the object store.

## Expected behaviour

A `PreparedWrite` (prepare, commit, abort by id) in the write set, journalled by `transactional`,
and at least one database storage model behind a feature.

## Discovery

Out of scope for `design/store-adapters/` (Phase 1 non-goals); interface left unimplemented.
