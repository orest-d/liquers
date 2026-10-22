# Phase 2: Solution & Architecture — Reading relational databases as record sources

Stops here: `readiness: phase2-blocked` (Phase 1, questions 1-2). This phase records what is
settled, the candidate shapes for what is not, and the slice that becomes buildable once the two
questions are answered.

## Overview

The read mechanics are settled by repository evidence:

- **Source type.** A `SqlTableSource` implements `liquers_records::RecordSource`
  (`liquers-records/src/value.rs`): `chunks()` returns `ChunkList::Unbounded { computed }`
  (`liquers-records/src/batch.rs`) — the count is unknown and a walk ends at the first short
  chunk, which is exactly keyset pagination; `stream()` opens a fresh traversal per call;
  `schema()` returns the schema discovered from the prepared statement. It is reached through
  `ExtValue::RecordSource` (existing variant), so `ns-rec`, `ns-search` and, after
  `ns-rec/materialize`, `ns-pl` (including `ns-pl/sql` from `sql-over-tables`) work unchanged.
- **Client.** `sqlx` (`query(…).fetch(&pool)` is a `BoxStream` of rows; one API for SQLite,
  PostgreSQL, MySQL; async on Tokio, so no blocking I/O). It is already in `Cargo.lock`, and
  `store-database-records` adds it to `liquers-store` with `["sqlite", "runtime-tokio"]`; this
  design uses the same version.
- **Chunks.** Keyset: `WHERE pk > $last ORDER BY pk LIMIT n`, with `n` a source option. The
  single-column primary key becomes the `KeyRole::Id` field, which `RecordSchema::new` already
  limits to one (`liquers-records/src/schema.rs`). No primary key, or a composite one: one chunk
  (no pagination), stated in the chunk descriptor.
- **Types.** `Bool`/`Int`/`Float`/`Text`/`Binary`/`Date`/`Timestamp` map directly
  (`TIMESTAMPTZ` → `Timestamp` UTC, decided 2026-10-10 for records; `TIMESTAMP` without zone →
  `Timestamp` read as UTC, with a warning). `NUMERIC`, `UUID`, `JSONB`, arrays, intervals,
  `TIME`: `Text` (JSON columns with `media_type: application/json`) and a warning, until
  `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` lands.

Rejected alternatives:
- **OpenDAL's `postgresql`/`mysql`/`sqlite` services** (`liquers-store` features): key-value blobs
  in one table, not access to existing tables.
- **`rusqlite`**: synchronous; would need `spawn_blocking` around every call, and gives SQLite only.
- **SurrealDB** as first backend: flat subset only (`engine-survey.md` §3, gap 4).
- **Writes in the same design**: split off (`NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE`).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `RECORD-FIELD-TYPE-HAS-NO-DECIMAL` | draft | P3 | AC-3 can only be met by text fallback until it lands | For a faithful release, yes | No (fallback) | Filed 2026-10-21 |
| `STORE-NO-DATABASE-STORAGE-MODEL` (`store-database-records`) | draft | P3 | Shares `sqlx`; no contract shared | No | No | Align versions/features |
| `NO-WRITE-PATH-FROM-RECORDS-INTO-A-RELATIONAL-DATABASE` | draft | P3 | Out of scope | No | No | Filed 2026-10-21 |

## Interfaces (candidate, not committed)

The settled part:

```rust
/// One table or one read-only statement of one database, read in keyset pages.
pub struct SqlTableSource {
    pool: sqlx::AnyPool,            // or a backend-specific pool per feature
    statement: SqlStatement,        // table name or SELECT text; never credentials
    key: Option<String>,            // keyset column (the Id field)
    page_rows: usize,
    schema: Arc<RecordSchema>,
    computed: Vec<ChunkId>,         // chunks known when built (ChunkList::Unbounded)
}
impl RecordSource for SqlTableSource { /* stream, chunks, describe_chunk, schema */ }
```

`chunks(&self)` returns `ChunkList<'_>` borrowing a slice, so chunks discovered by a later walk
cannot be reported through it from behind a lock (a guard cannot outlive the call). As in the
existing unbounded source (`liquers-records/src/sources.rs`, `ChunkList::Unbounded { computed:
&self.ids }`), `computed` is what is known when the source value is built — the first page's id,
or nothing; reconciliation is append-only either way. Reporting walked chunks would need an owned
`ChunkList` or a new source value per walk, a `liquers-records` change to decide at Phase 2 resume.

The blocked part — how the source is obtained — has three candidate shapes (Phase 1, question 1):

| Shape | Spelling | For | Against |
|---|---|---|---|
| A. database mount | `-R/pg/sales/orders` (store type `sql`, config with `${VAR}` credentials) | config, credentials and routing reuse `STORE_CONFIG_FSD.md`; "everything is an asset" | `AsyncStore::get` returns bytes: a table would be serialized whole, losing laziness, unless the store gains a typed-value path (core change) |
| B. connection value | `-R/config/pg.yaml/-/ns-db/connect/table-orders` | lazy, typed; no core change | credentials in the store; a non-serializable `ExtValue` (a fifth step to the value-type procedure: no `TypeInfo` format) |
| C. environment section | `ns-db/table-sales-orders`, `databases:` in `EnvironmentConfig` | credentials beside store credentials; short queries | a core config field for a lib feature (VISION "core does not grow rich features"), unless the section is an opaque extension map |

Freshness (question 2) is orthogonal to the shape: (a) `volatile: true`, (b) `expires:` per source,
(c) a backend change marker as the chunk's version in `describe_chunk` (`ChunkDescriptor`
metadata), which `external-index-sync`'s version diff could reuse.

## Integration Points

Settled regardless of the answers: a native-only feature (`sql-sources`, name to decide) in the
chosen crate; `ExtValue::RecordSource` reused; no change to `liquers-records`, `liquers-web` or
`liquers-py` beyond feature forwarding. Undecided: `liquers-store/src/store_factory.rs` (shape A),
`liquers-lib/src/value/` (shape B), `liquers-core/src/environment_builder.rs` (shape C).

**Crate placement (maintainer decision):** `liquers-lib` behind a feature is the default place for
"batteries"; a new `liquers-sql` crate (on `liquers-records` + `liquers-core`) keeps database
clients out of `liquers-lib` builds and lets `liquers-axum` use it without the rich library.
Recommended: the new crate, given sqlx's weight (OpenDAL's database services are counted at
+104 crates together in `liquers-store/Cargo.toml`).

## Error Handling

Connection failure → `Error::general_error` with the database's *name* (never the URL);
unsupported column type → a warning in the result's metadata log, or `Error::conversion_error_with_message`
when refusal is chosen; feature off → `Error::not_supported` naming the feature (as
`store-database-records` does for `sqlite-store`).

## Relevant Commands

Depends on question 1: shape B or C adds an `ns-db` namespace (`table`, `sql` taking a link to a
stored `.sql` file as `sql-over-tables` does); shape A adds none. Namespaces interacting: `rec`
(and `pl` after `materialize`).

## Documentation Architecture

Deferred with Phases 3-4: new `specs/reference/RELATIONAL_SOURCES.md`; `RECORD_STREAMS.md`
(source kinds); `ENVIRONMENT_CONFIG.md` or `STORE_CONFIG_FSD.md` by shape. `affects_docs` stays
empty until a shape is chosen.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | new crate or `liquers-lib/src/sql/`; one of the three integration sites above |
| Crates and workflows affected | `liquers-records` consumers; deployments opting in; `check-build-matrix.sh` (new feature, native-only) |
| Existing tests likely to change | None for shape B; store factory availability tests for A; environment config tests for C |
| New validation | Conformance tests against SQLite (in-process, no server); PostgreSQL tests behind an env var |
| Compatibility / data / concurrency / performance / security | Credentials leakage (AC-5); exactness of types (AC-3); pool lifetime vs asset lifetime; disk/build cost of sqlx; long-running reads hold a connection per open stream |
| Recovery | Feature off; nothing else depends on it |
| Certainty and open questions | High on mechanics; **blocked** on identity and freshness, which change the query spelling, the cache behaviour and possibly core config |
