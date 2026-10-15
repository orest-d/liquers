# Phase 4: Implementation Plan — store-adapters

## Overview

The work is built bottom-up, so every step compiles and keeps today's tests green on its own. No
existing behaviour changes until step 9 wires configuration into the builder, and even then only for
mounts that have a `read_only` or `metadata` section (AC-6).

The order:
1. Record fields and data-only access (steps 1–3).
2. The four parts, each unit-tested in isolation: decorator, realm compiler, storage models, write
   strategies (steps 4–7).
3. The pipeline that joins them (step 8).
4. Configuration (step 9), then conformance (step 10).
5. Crates above core (steps 11–15).

Steps 4–7 depend only on steps 1–3 and can run in parallel. Steps 13–15 depend on step 9 and can
also run in parallel.

Prerequisites: none. The preflight (Phase 2) found no blocking issue.

## Progress
- [ ] Step 1: `created` and `etag` on records, `BackendTime`
- [ ] Step 2: data-only access on `AsyncStore`, with refusing defaults
- [ ] Step 3: data-only access in the memory, file and router stores
- [ ] Step 4: `ReadOnlyStore`
- [ ] Step 5: realm `store_config`: compiler and built-in handlers
- [ ] Step 6: storage models
- [ ] Step 7: write strategies and failure injection
- [ ] Step 8: `MetadataPipelineStore`
- [ ] Step 9: configuration types, defaults, `build_store`
- [ ] Step 10: conformance suites `C7`–`C12`
- [ ] Step 11: `front_matter` and `front_matter_sync` in `liquers-lib`
- [ ] Step 12: realm entries in `specs/command_registry.yaml`
- [ ] Step 13: OpenDAL data-only access
- [ ] Step 14: HTTP 501 test and Python getters
- [ ] Step 15: `liquers-web`: `isReadOnly` and the optional data-only methods

## Implementation Steps

### Step 1: `created` and `etag` on records, `BackendTime`
- Files / symbols: `liquers-core/src/metadata.rs`: `MetadataRecord`, `AssetInfo`, new
  `BackendTime`, `TimePrecision`.
- Change:
  - Add `pub created: Option<BackendTime>` and `pub etag: Option<String>`, each with
    `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  - `BackendTime { time: chrono::DateTime<Utc>, precision: TimePrecision }` serializes as one RFC
    3339 string truncated to its precision.
  - `AssetInfo` copies both fields.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib metadata`. Then
  `older_build_reads_new_record_as_legacy` (written now, against a fixture parsed with the
  pre-change `MetadataRecord` shape) and a round-trip test, `backend_time_round_trips_at_its_precision`.
- Rollback: revert the commit. The fields are additive and skipped when `None`.

### Step 2: data-only access on `AsyncStore`, with refusing defaults
- Files / symbols: `liquers-core/src/store.rs`: `AsyncStore`; new `DataCapabilities`, `DataFacts`,
  `ReservedNameSet`.
- Change:
  - Add the Phase 2 §1 methods, each defaulting to `Error::not_supported`, or `false` / empty for
    the query methods: `get_data`, `set_data`, `remove_data`, `rename_data`, `listdir_data`,
    `data_facts`, `get_stored_metadata`, `data_capabilities`, `is_read_only`, `sync`,
    `version_verification`.
  - Add `ReservedNameSet`, with the predicate it shares with `ReservedNames`.
- Depends on: step 1.
- Proof: `cargo test -p liquers-lib --lib --tests` (unchanged); `C1`–`C6` unchanged
  (`cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`);
  `bash scripts/check-build-matrix.sh` for the wasm32 trait bounds.
- Rollback: revert. All methods are defaulted, so no implementor changes.

### Step 3: data-only access in the memory, file and router stores
- Files / symbols: `store.rs` `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`.
- Change:
  - The file store's data-only methods read and write the file alone: no sidecar, no repair on read.
    `rename_data` is `tokio::fs::rename`, and `data_facts` comes from `metadata()`, at nanosecond
    precision where the platform gives it.
  - The memory store keeps an etag counter.
  - The router forwards each data-only method to the selected member, with the existing
    `find_store`.
- Depends on: step 2.
- Proof: unit tests `data_only_get_does_not_create_sidecar`,
  `data_facts_change_when_file_is_rewritten`, `router_forwards_data_only_methods`.
- Rollback: revert. Nothing calls these methods yet.

### Step 4: `ReadOnlyStore`
- Files / symbols: new `liquers-core/src/store/read_only.rs`, declared from `store.rs` the way
  `escape/` is.
- Change: `pub struct ReadOnlyStore { inner: Arc<dyn AsyncStore> }`. It implements every
  `AsyncStore` method:
  - writes return `not_supported`;
  - reads forward to `inner`, metadata through `inner`'s data-only path where it has one, so that a
    read cannot create a sidecar (AC-1);
  - `is_read_only` returns `true`.
- Depends on: step 3. Parallel with 5–7.
- Proof: the three `read_only.rs` tests in Phase 3.
- Rollback: delete the module.

### Step 5: realm `store_config`: compiler and built-in handlers
- Files / symbols: new `liquers-core/src/store/store_config.rs` (the Phase 2 §4 types, `fill_realm`,
  `StoreConfigFunctions::compile`, `register_builtin_store_config`) and `store/handlers.rs` (the
  catalogue's handlers); `commands.rs` `CommandRegistry`, which gains `store_config:
  StoreConfigFunctions`; the executor, which refuses realm `store_config` with `not_supported`.
- Change: as Phase 2 §4.
  - Every `Step` variant is matched explicitly.
  - Read-stage steps are wrapped so an error becomes a log warning.
  - The `CommandMetadata` of each entry is built in code next to its builder.
- Depends on: step 1. Parallel with 4, 6 and 7.
- Proof: `cargo test -p liquers-core --lib store::store_config`, which runs every `store_config.rs`
  test of Phase 3.
- Rollback: revert. Registration is additive and the executor change affects only the new realm.

### Step 6: storage models
- Files / symbols: new `store/storage.rs`: `MetadataStorage`, `NativeStorage`, `PerEntryStorage`,
  `Layout`, `GroupedStorage`, `NoStorage`, `MemoryCentralStorage` (under `cfg(any(test, feature =
  "store-conformance"))`). Their `ComponentBuilder`s are registered in
  `register_builtin_store_config`.
- Change: Phase 2 §3. The grouped file is a `BTreeMap<String, MetadataRecord>`, serialized as JSON,
  so writes are deterministic.
- Depends on: steps 3 and 5.
- Proof: the `storage.rs` tests.
- Rollback: revert the module and its registrations.

### Step 7: write strategies and failure injection
- Files / symbols: new `store/writes.rs`: `WriteStrategy`, `WriteSet`, `ObjectWrite`, `WriteTarget`,
  `PreparedWrite`, `Recovery`, `SyncReport`, `DirectWrite`, `AtomicWrite`, `JournalWrite`.
  `store_conformance/failing.rs` holds `FailingStore` (fails the *n*-th data-only call, then stops),
  behind the same cfg.
- Change: Phase 2 §5. The journal is JSON listing every object with its staged key.
- Depends on: steps 3 and 5. Parallel with 6.
- Proof: the `writes.rs` tests, with each strategy over `FailingStore` for every *n* up to the
  number of calls a commit makes.
- Rollback: revert.

### Step 8: `MetadataPipelineStore`
- Files / symbols: new `store/pipeline.rs`: `MetadataPipelineStore`, `PipelineParts`,
  `MetadataCache`, `GroupLocks`; central storage with backups.
- Change: the Phase 2 §6 flows.
  - Locks are an `scc::HashMap<Key, Arc<tokio::sync::Mutex<()>>>`, evicted when `Arc::strong_count`
    is 1 after release.
  - The cache is keyed by key and stores `(MetadataRecord, DataFacts)`.
- Depends on: steps 4–7.
- Proof: the `pipeline.rs` tests.
- Rollback: revert. Nothing builds it until step 9.

### Step 9: configuration types, defaults, `build_store`
- Files / symbols:
  - `store_config.rs` (the configuration file at the crate root): `StoreConfig` and
    `StoreRouterConfig` gain `read_only` and `metadata`; new `MetadataConfig` and
    `METADATA_DEFAULTS`.
  - `store_factory.rs`: `StoreRouterBuilder::with_store_config(&CommandMetadataRegistry,
    &StoreConfigFunctions)` and `build_store`. `build_without_env_expansion` wraps each mount.
  - `environment_builder.rs` `build` calls `build_store` with its command registry.
  - `liquers-store`'s `create_router*` and `liquers-web`'s `build_router` use
    `StoreConfigFunctions::builtin()`.
- Change: Phase 2 §7 and §Configuration. A non-empty old `metadata` map is refused, naming the
  field.
- Depends on: step 8.
- Proof: `cargo test -p liquers-core --features store-conformance --test store_adapters`, then the
  default loop `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert. Configurations without the new fields are unaffected.

### Step 10: conformance suites `C7`–`C12`
- Files / symbols: `liquers-core/tests/store_conformance_CONF.rs`; `specs/reference/STORE_SEMANTICS.md`'s
  status table, if it is generated from the suites.
- Change: add the fixtures listed in Phase 3. A suite that fails a rule because of a known issue
  lists it as `AllowedFailure` naming the issue.
- Depends on: step 9.
- Proof: `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`.
- Rollback: remove the suites.

### Step 11: `front_matter` and `front_matter_sync` in `liquers-lib`
- Files / symbols: new `liquers-lib/src/store_config.rs`: the handlers and
  `register_lib_store_config`, called from `liquers-lib/src/environment.rs` wherever it builds its
  command registry.
- Change: the catalogue rows. The parse reuses `liquers_records::search::front_matter_end` behind
  feature `records`. Without that feature the two entries are not registered, and naming them is
  the ordinary unknown-entry error.
- Depends on: step 9.
- Proof: `cargo test -p liquers-lib --lib store_config`; `bash scripts/check-build-matrix.sh`.
- Rollback: revert.

### Step 12: realm entries in `specs/command_registry.yaml`
- Files / symbols: `liquers-lib` `export-command-registry`; `specs/command_registry.yaml`;
  `liquers-validate` (`liquers-core/src/validate/`), only if a realm other than `""` needs a change
  there.
- Change: regenerate, with a CHANGELOG line. Check that `liquers-validate` plans
  `-store_config/front_matter` against the exported registry.
- Depends on: step 11.
- Proof: `cargo test -p liquers-lib --test registry_export`; `liquers-validate -- '-store_config/media_type/file_facts'`
  exits 0.
- Rollback: regenerate from the previous commit.

### Step 13: OpenDAL data-only access
- Files / symbols: `liquers-store/src/opendal_store.rs`.
- Change: the data-only methods over `opendal::Operator`. `rename_data` is offered where the
  service supports rename, `data_facts` comes from `stat`, and the etag comes from the service where
  it has one.
- Depends on: step 2. Parallel with 14 and 15.
- Proof: `cargo test -p liquers-store`, plus a `C9` variant over `opendal_fs` if the suite runs it
  today.
- Rollback: revert. The trait defaults refuse again.

### Step 14: HTTP 501 test and Python getters
- Files / symbols: `liquers-axum/tests/store_api_integration.rs`; `liquers-py/src/` (the
  `MetadataRecord` wrapper).
- Change: the test `read_only_store_write_is_501`; getters for `created` and `etag`.
- Depends on: step 9.
- Proof: `cargo test -p liquers-axum --test store_api_integration`; `cargo check -p liquers-py`.
- Rollback: revert.

### Step 15: `liquers-web`: `isReadOnly` and the optional data-only methods
- Files / symbols: `liquers-web/src/store/` (`JsStore`, `LiquersStore`); `liquers-web/README.md`;
  the stub declarations.
- Change:
  - `JsStore` looks up `getData`, `setData`, `removeData`, `renameData`, `listdirData` and
    `dataFacts` once, at adaptation, and reports `data_capabilities` from what it finds.
  - `LiquersStore.isReadOnly()`.
  - The built-in browser stores keep refusing data access.
- Depends on: step 9.
- Proof: the four `store_js_STORE.rs` tests;
  `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`; then
  `./liquers-web/scripts/check-stubs.sh` (after `cargo clean`, per `CLAUDE.md`).
- Rollback: revert. A store object without the methods behaves as before.

## Testing Plan

Each step runs its own proof and then the default loop (`cargo test -p liquers-lib --lib --tests`),
so a step that breaks something elsewhere stops there.

Additional runs:
- `bash scripts/check-build-matrix.sh` after steps 2, 9 and 11, which touch trait bounds, `cfg`
  features and optional dependencies.
- After step 15: the whole of `liquers-core` with `--features store-conformance`, `liquers-store`,
  `liquers-lib`, `liquers-axum`, and the `liquers-web` Node loop, each separately and with `cargo
  clean` between where disk requires.

The known flaky test `volatile_dependency_has_one_value_per_evaluation`
(`EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`) is re-run once if it fails, and is not treated as a
regression.

## Rollback Plan

Every step is a separate commit and reverts on its own. Until step 9 nothing is reachable from
configuration. If only steps 1–8 land, the code is unused but tested, and the remainder becomes an
issue (§5.6). If step 15 cannot be finished, the JavaScript half is filed as an issue and Phase 5
records it; the Rust stores are complete without it.

## Documentation Updates

Create:
- `specs/reference/REALMS.md`.
- `specs/guides/STORE_CONFIG_GUIDE.md`.

Update:
- `STORE_SEMANTICS.md`.
- `STORE_CONFIG_FSD.md` (the defaults table).
- `STORE_IMPLEMENTATION_GUIDE.md`.
- `STORE_FACTORY_GUIDE.md`.
- `ASSETS.md`.
- `PROJECT_OVERVIEW.md` (its realm section points to `REALMS.md`).
- `LANGUAGE-INTEGRATION_GUIDE.md`, following the Phase 2 plan, with `STORE14`–`STORE18` in
  Appendix A.
- `CLAUDE.md` "Adding a Store Backend".
- `liquers-web/README.md`.
- `.claude/skills/liquers-validate/SKILL.md`.

Every updated `reference/` or `guides/` document gets a `## History` row and a bumped `reviewed:`.

Regenerate and register:
- `specs/command_registry.yaml` (step 12).
- `specs/index.csv` after every front-matter change.
- `specs/README.md`: a capability-map entry, and the design marked complete at Phase 5.
- Add the two new documents to `affects_docs` when they exist.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
