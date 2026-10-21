# Phase 2: Solution & Architecture — Metadata records in a database

## Overview

`AsyncSqliteStore` is an ordinary `AsyncStore` over one SQLite table of objects (key → bytes,
stored record, modification time, ETag), with every data-only method and honest
`DataCapabilities`. A record store in a database is then configuration, not a new mechanism: a
router-level `metadata: { storage: central-<mount> }` names a `sqlite_kv` mount, and the existing
write strategies stage, journal and recover across the data store and that mount.

Rejected, one line each:
- **`PreparedWrite` in `WriteSet`** (the source's plan, `store-adapters` Phase 2 §5): the journal
  already lists objects per target store and moves them with `rename_data` or copy-and-delete
  (`JournalWrite::commit` / `roll_forward`, `writes.rs`); a store that renames atomically needs no
  second protocol. Q2.
- **A `MetadataStorage` that owns a connection** (`database-<path>`): duplicates central storage,
  puts a file path into a spec (`/` escaped as `~I`), and cannot be checked by the store suites.
- **OpenDAL's `sqlite` service**: no `list` capability, so no listing, no record-only keys, no
  recovery scan.
- **`rusqlite` / `redb`**: Q1.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `STORE-CENTRAL-STORAGE-MODEL-NOT-IMPLEMENTED` | draft | P3 | AC-3, AC-4 need `central-<mount>` | yes (`central-storage-model`) | no for AC-1/2/6 | order steps |
| `STORE-WRITE-HAS-NO-PRECONDITION` | draft | P2 | SQLite serializes writers across processes; the pipeline's group lock stays per process | no | no | note in §13 text |
| `NO-RELATIONAL-DATABASE-ACCESS-LAYER` | draft | P2 | engine choice should agree | no | no | Q1 |

## Interfaces

`liquers-store/src/sqlite_store.rs` (feature `sqlite-store`, native only):

```rust
pub struct AsyncSqliteStore { pool: sqlx::SqlitePool, table: String, prefix: Key }
impl AsyncSqliteStore {
    pub async fn open(path: &str, table: &str, prefix: Key) -> Result<Self, Error>;
}
#[async_trait] impl AsyncStore for AsyncSqliteStore { /* all methods below */ }

/// Declares `sqlite_kv`; unavailable, naming the feature, when it is off.
pub struct SqliteStoreFactory;
impl StoreFactory for SqliteStoreFactory { /* store_types, create */ }
```

Table (created when absent; the name is validated as `[A-Za-z_][A-Za-z0-9_]*`):

```sql
CREATE TABLE IF NOT EXISTS <table> (
  key TEXT PRIMARY KEY, data BLOB, metadata TEXT,
  modified_ns INTEGER NOT NULL, etag INTEGER NOT NULL)
```

`data IS NULL` is a key with a record and no data object (§8: enumerable, `get` is `KeyNotFound`).
The pool opens in WAL mode with `create_if_missing`.

| Method | SQL / behaviour |
|---|---|
| `get_data`, `data_facts` | `SELECT data` / `length(data), modified_ns, etag` by key; `NULL` data → `KeyNotFound` / `None` |
| `set_data` | `INSERT … ON CONFLICT(key) DO UPDATE SET data, modified_ns = now, etag = etag + 1` (one statement: atomic) |
| `remove_data` | `UPDATE … SET data = NULL`, then `DELETE` when `metadata IS NULL` |
| `rename_data` | one transaction: `DELETE` the target, `UPDATE key` of the source (atomic) |
| `listdir_data(folder)` | `SELECT key WHERE key >= '<folder>/' AND key < '<folder>0'` (`'0'` is the byte after `/`), first segment after the folder, deduplicated: derived directories |
| `get_stored_metadata`, `set_metadata` | the `metadata` column (JSON as the file store writes sidecars) |
| `get`, `set`, `remove`, `is_dir`, `listdir`, `contains`, `removedir`, `keys` | the ordinary methods over the same rows; `set` writes data and record in one statement; `is_dir` = some key below; `removedir` = one `DELETE` of the range |
| `data_capabilities` | all five `true`; times through `BackendTime::new(t, TimePrecision::Nanos)` |
| `makedir` | default (`KeyNotSupported`): directories are derived only |

`StoreTypeInfo` for `sqlite_kv`: arguments `path` (string, required; `:memory:` allowed) and
`table` (string, default `liquers_objects`), `ArgumentCoverage::Complete`.
`liquers-store/src/store_factory.rs` `default_store_factory` chains `SqliteStoreFactory` after
`OpendalStoreFactory` (no name collides). `create` is synchronous (`StoreFactory::create`), so the
pool is opened lazily (`SqlitePoolOptions::connect_lazy_with`) and the table is created on first use
under a `tokio::sync::OnceCell`; a bad path therefore fails at first access with the store's
`key_read_error`/`key_write_error`, and `create` validates only the arguments.

Sync/async: all I/O is async through `sqlx` (Q1), so no blocking call runs on the runtime.

## Integration Points

- `liquers-store/Cargo.toml`: `sqlx = { version = "0.8", default-features = false, features =
  ["sqlite", "runtime-tokio"], optional = true }` (the version already locked), feature
  `sqlite-store = ["dep:sqlx", "async_store"]`, and tokio as a dependency for `OnceCell`.
- `liquers-store/src/lib.rs`: `#[cfg(feature = "sqlite-store")] pub mod sqlite_store;` and the
  factory declared in both feature states.
- `liquers-store/tests/store_conformance_CONF.rs`: suites `c8_sqlite` (plain) and
  `a3_sqlite_per_entry` (metadata section, `atomic` and `transactional`), `a5_sqlite_central`
  (records of a memory member in a `sqlite_kv` mount), each with a `status_table` row.
- `scripts/check-build-matrix.sh` gains the `sqlite-store` row for `liquers-store` (script edit).

## Error Handling

| Failure | Constructor |
|---|---|
| missing `path`, bad `table` name | `Error::configuration_error` at `create`, naming the mount |
| feature off | `Error::not_supported` naming `sqlite-store` (through `StoreTypeAvailability::Unavailable`) |
| SQL error on read / write | `Error::key_read_error` / `Error::key_write_error` with the key and store name |
| relative or reserved key | `KeyNotAbsolute` first, then `KeyNotSupported` (the store reserves nothing itself) |
| corrupt `metadata` JSON | kept as `LegacyMetadata` (`decode_metadata`), as other stores |

## Relevant Commands

None. No command namespace and no `store_config` entry; `specs/command_registry.yaml` unchanged.

## Documentation Architecture

- `specs/reference/STORE_CONFIG_FSD.md`: a "SQLite object store (`sqlite_kv`)" subsection under
  Supported Store Types, and the feature in "Which services a build has".
- `specs/reference/STORE_SEMANTICS.md`: §11 capabilities table column; §13 a sentence that
  central storage over a `sqlite_kv` mount keeps records in a database.
- `specs/guides/STORE_CONFIG_GUIDE.md` §9: the Problem Example as a third complete mount.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9: status row.
- `affects_docs` as in `DESIGN.md`; no `specs/README.md` change.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-store/Cargo.toml`, `src/lib.rs`, `src/sqlite_store.rs` (new), `src/store_factory.rs`, `tests/store_conformance_CONF.rs`; `scripts/check-build-matrix.sh` |
| Crates and workflows affected | `liquers-store` only; deployments opting into the feature |
| Existing tests likely to change | `availability02_advertised_types_match_the_enabled_features` and `default01_chain_is_core_then_opendal` gain the new type / chain member |
| New validation | three conformance suites, unit tests of listing bounds and key escaping, build matrix row |
| Compatibility / data / concurrency / performance / security | new type only; SQLite file format is stable; WAL allows readers during a write, one writer at a time across processes; a `LIKE`-free range keeps `%`/`_` keys literal; the table name is validated, everything else is bound as parameters (no SQL injection) |
| Recovery | disable the feature; nothing else references the module |
| Certainty and open questions | high for the mechanism (cross-store journalling read in `writes.rs`); Q1 decides the dependency |
