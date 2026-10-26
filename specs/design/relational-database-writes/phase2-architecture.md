# Phase 2: Solution & Architecture — Writing records into a relational database

Stops here: `readiness: phase2-blocked` (Phase 1, questions 1-2). This phase records the settled
mechanics of the recommended form, the candidate shapes of what is not settled, and what becomes
buildable once both questions are answered.

## Overview

Under form A, a `SqlMirrorSink` implements `liquers_records::sync::IndexSink` over one table. The
reconciliation contract drives it: `observed()` reads a side table, `apply()` replaces each upserted
chunk and deletes each removed one, one transaction per chunk. Nothing in Liquers' query language,
assets or commands changes.

```rust
// in the crate chosen by relational-database-access (feature `sql-sources` or its successor)
pub struct SqlMirrorSink {
    pool: sqlx::Pool<…>,            // backend per feature (SQLite first, PostgreSQL)
    table: SqlTableName,            // schema-qualified; validated identifier, never interpolated raw
    sync_table: SqlTableName,       // `<table>_liquers_sync`
    columns: Vec<ColumnMapping>,    // field → column, type checked against RecordSchema
    projection: ProjectionId,       // hash of table name + column mapping + type mapping version
    refresh: Expires,
}
impl IndexSink for SqlMirrorSink { … }
```

- **`observed`**: `SELECT chunk, freshness_kind, version, fed_at, projection FROM <sync_table>`;
  `version` stored as `Version`'s `Display` text (it already implements `Serialize`/`Display`),
  `fed_at` as UTC timestamp; `ChunkId` as its serde JSON form (it derives `Serialize`).
- **`apply`**: for each upsert, `BEGIN; DELETE FROM <table> WHERE _liquers_chunk = $1; INSERT …
  (batched rows, bound parameters); UPSERT the sync row; COMMIT`; for each deletion, the same
  `DELETE` plus deleting the sync row, in its own transaction. Idempotent by construction (the
  contract's requirement): replaying an upsert deletes and re-inserts the same rows.
- **`field_support`**: `Exact` for every column with an exact mapping; `Unsupported` for a field
  that has no column (a `Similarity`/`Vector` field); never `Inexact` (a mirror answers SQL, not
  the search predicate).
- **Schema**: on the first `apply`, the table's columns (from the database catalog) are checked
  against `columns`; mismatch → `Err` before any write (AC-5). Creation only when configured
  (question 4).
- **Types**: `Bool`/`Int`/`Float`/`Text`/`Binary`/`Date`/`Timestamp` map directly; `Vector` and any
  field without an exact column type → refused naming it (question 5).

Rejected alternatives (Phase 1, question 1): write commands (B) — non-reproducible queries, VISION
non-goals; a writable database store mount (C) — the store contract is bytes per key.
Also rejected: `MERGE`/`ON CONFLICT` row upserts keyed by a record `Id` — a record has no version of
its own and chunks are replaced wholesale by the contract; row-level merge would need a second
identity regime.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-RELATIONAL-DATABASE-ACCESS-LAYER` (`relational-database-access`) | draft, phase2-blocked | P2 | database identity, credentials, client, crate | yes | **yes** | `requires` |
| `EXTERNAL-INDEX-HAS-NO-RECONCILIATION-CONTRACT` (`external-index-reconciliation`) | draft, needs-decision | P2 | the `IndexSink` contract | yes | no (designed) | `requires` |
| `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` | draft | P3 | exact numeric columns | for full fidelity | no | refuse until then |
| `UNKEYED-ASSET-HAS-NO-VERSION` | draft | P3 | unkeyed chunks are mirrored time-based | no | no | none |

## Interfaces (candidate, not committed)

The settled part is the sink above. The blocked part is configuration — where the pool, table and
mapping come from:

| Shape (from `relational-database-access` Q1) | Mirror configuration |
|---|---|
| A. database mount | a `mirror:` section on the store mount naming source query, table and refresh |
| B. connection value | a command `ns-db/mirror` taking a connection link and a source — but a command that writes is form B of this design's question 1, so it would have to be a non-query entry point (a CLI or server task) instead |
| C. environment section | `databases.<name>.mirrors[]` read by a server task that calls `reconcile` on a schedule |

Who runs the reconciliation (a server task, a CLI, a scheduler built on `ExpirationMonitor`) is the
same question for every external sink and is left to the first real sink of
`external-index-reconciliation`.

## Integration Points

Settled: one new type in the database crate; no change to `liquers-core`, `liquers-records`,
`liquers-web` or `liquers-py`; a native-only feature in `check-build-matrix.sh`. Undecided: where
mirror configuration lives (table above).

## Error Handling

Connection or transaction failure → `Error::general_error` naming the database by its configured
name, never the URL (AC-6); schema mismatch and unsupported types → `Error::conversion_error_with_message`
naming the column, before any write; a failed chunk transaction rolls back and is reported by
`reconcile` as `Failed` for that chunk only (the contract's AC-7).

## Relevant Commands

None under form A. Form B would add `ns-db/insert`, `update`, `delete` (not recommended).

## Documentation Architecture

Deferred with Phases 3-4: a "Mirroring a source" section in the relational reference and the
record stream guide. `affects_docs` stays empty until the blocking questions are answered.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the database crate's new `mirror.rs`; its feature list; tests against SQLite |
| Crates and workflows affected | deployments that configure a mirror; none otherwise |
| Existing tests likely to change | none |
| New validation | `IndexSink` scenarios of `external-index-reconciliation` re-run against SQLite; a transaction-visibility test (a reader during `apply` sees the old or new chunk, never half) |
| Compatibility / data / concurrency / performance / security | Liquers must own the table (other writers' rows in a known chunk are deleted); SQL injection — table and column names validated as identifiers and quoted, values always bound; credentials (AC-6); large chunks mean large transactions |
| Recovery | drop the mirror configuration; the table stays as last written |
| Certainty and open questions | high on mechanics given the contract; **blocked** on the scope decision (question 1) and the inherited identity question (question 2), which change whether any of this is built and how it is configured |
