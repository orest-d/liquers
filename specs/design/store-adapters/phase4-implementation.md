# Phase 4: Implementation Plan — store-adapters

## Overview

The work is built bottom-up, so each step compiles and keeps the default loop green. Until step 10
wires configuration into the builder, no observable behaviour changes. Two exceptions:
- the planner fix of step 0;
- realm `store_config` appearing in registries and listings from step 5, which is regenerated in
  that same step.

Even after step 10, only mounts with `read_only` or `metadata` change (AC-6).

The order:
1. The planner fix and the record and trait foundations (steps 0–3).
2. The parts: decorator, realm compiler, storage models (steps 4–6).
3. Write strategies and the pipeline that joins the parts (steps 7–8).
4. The asset-manager hook (step 9), configuration (step 10), then conformance (step 11).
5. The crates above core (steps 12–15).

Steps 4 and 5 can run in parallel after step 3. Steps 13, 14 and 15 can run in parallel after
step 10.

Prerequisites: none outside this design. The planner defect, fixed in step 0, is part of it (Phase 2
§Configuration).

## Progress
- [x] Step 0: the planner applies a segment's realm to every action (a8e457f)
- [x] Step 1: `created`, `etag` and `BackendTime` on records (9b6bfdc)
- [x] Step 2: data-only access on `AsyncStore`, `ReservedNames` prefixes, test support (bcc3928)
- [ ] Step 3: data-only access in the memory, file and router stores
- [ ] Step 4: `ReadOnlyStore`
- [ ] Step 5: realm `store_config`: compiler, built-in handlers, registry export
- [ ] Step 6: storage models
- [ ] Step 7: write strategies
- [ ] Step 8: `MetadataPipelineStore`
- [ ] Step 9: the asset manager reads `version_verification`
- [ ] Step 10: configuration types, defaults, `build_store`
- [ ] Step 11: conformance suites `A1`–`A7`
- [ ] Step 12: `front_matter` and `front_matter_sync` in `liquers-lib`
- [ ] Step 13: OpenDAL data-only access
- [ ] Step 14: HTTP 501 test and Python getters
- [ ] Step 15: `liquers-web`: `isReadOnly`, the optional data-only methods, declarations

## Implementation Steps

### Step 0: the planner applies a segment's realm to every action
- Files / symbols: `liquers-core/src/plan.rs`: `PlanBuilder` (`process_query`, `process_action`,
  `get_command_metadata`, `promote_relative_default_links`).
- Change: when the builder splits a query into predecessor and action, it carries the realm of the
  transform segment the action came from, instead of re-deriving it from the reduced query.
- Depends on: none.
- Proof: the four `plan.rs` tests of Phase 3. All `liquers-core` tests and the default loop stay
  green, so realm `""` plans unchanged. `liquers-validate` plans `-r/a/b` against an overlay with
  realm `r`.
- Rollback: revert. Nothing else depends on it until step 5.

### Step 1: `created`, `etag` and `BackendTime` on records
- Files / symbols: `liquers-core/src/metadata.rs`: `MetadataRecord`, `AssetInfo`; new
  `BackendTime(String)` and `TimePrecision`, as in Phase 2 §1.
- Change:
  - Add `created: Option<BackendTime>` and `etag: Option<String>`, both `#[serde(default,
    skip_serializing_if = "Option::is_none")]`. `AssetInfo` copies them.
  - `BackendTime::new` formats an RFC 3339 UTC string truncated to the given precision.
  - The compatibility check compares a record's serialized keys with and without the new
    fields, rather than a committed fixture: both fields are skipped when `None`, so a record
    without them serializes exactly as before.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib metadata::` runs the three `metadata.rs` tests.
- Rollback: revert. Both fields are additive.

### Step 2: data-only access on `AsyncStore`, `ReservedNames` prefixes, test support
- Files / symbols:
  - `store.rs`: `AsyncStore`; new `DataCapabilities`, `DataFacts`, `SyncReport`; `ReservedNames`
    gains `prefixes` and `reserves(segment)`; new owned `ReservedNameSet`; new public
    `SharedStore`.
  - New `store/test_support.rs`: `RecordingStore`, `FailingStore`.
- Change:
  - The Phase 2 §1 methods, all defaulted.
  - `version_verification` returns `None`, and `assets.rs` is untouched until step 9.
- Depends on: step 1.
- Proof:
  - `cargo test -p liquers-lib --lib --tests`.
  - `C1`–`C6` unchanged: `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`.
  - `bash scripts/check-build-matrix.sh`, for the wasm32 bounds.
- Rollback: revert. No implementor has to change.

### Step 3: data-only access in the memory, file and router stores
- Files / symbols: `store.rs` `AsyncMemoryStore`, `AsyncFileStore`, `AsyncStoreRouter`.
- Change:
  - The file store reads and writes the file alone: no sidecar, no repair.
    - `rename_data` uses `tokio::fs::rename`.
    - `data_facts` comes from `tokio::fs::metadata`, at nanosecond precision.
    - `get_stored_metadata` reads the sidecar without repairing it.
  - The memory store keeps a modification time and an ETag counter.
  - The router forwards to the member it selects, answers `is_read_only` as "it has members and
    all of them are", and reports the intersection of its members' capabilities.
- Depends on: step 2.
- Proof: the five `store.rs` tests of Phase 3.
- Rollback: revert. Nothing calls these methods yet.

### Step 4: `ReadOnlyStore`
- Files / symbols: new `store/read_only.rs`, declared from `store.rs` the way `escape/` is.
- Change: Phase 2 §2.
  - Over a store with data access, a missing record is synthesized by today's `default_metadata`
    plus `finalize_metadata(update = false)`, with no chain.
  - Any other store is forwarded unchanged.
- Depends on: step 3. Can run in parallel with step 5.
- Proof: `cargo test -p liquers-core --lib store::read_only` runs the three `read_only.rs` tests.
- Rollback: delete the module.

### Step 5: realm `store_config`: compiler, built-in handlers, registry export
- Files / symbols:
  - New `store/realm.rs`: the Phase 2 §4 types, `fill_realm`, `StoreConfigFunctions::{builtin,
    compile}`, `register_builtin_store_config` and `register_test_store_config`.
  - New `store/handlers.rs`: the handler builders.
  - `commands.rs`: `CommandRegistry` gains `store_config: StoreConfigFunctions`, `new()` calls
    `register_builtin_store_config`, and the executor refuses the realm with `not_supported`.
  - `command_metadata.rs`: `NON_EXECUTABLE_REALMS` and
    `CommandMetadataRegistry::executable_commands()`.
  - `liquers-lib/src/bin/export_command_registry.rs` `build_registry` and
    `liquers-lib/tests/registry_export.rs` `full_registry`: the built-in entries, as group
    `store_config`.
  - `specs/command_registry.yaml`, regenerated with a CHANGELOG line.
- Change: Phase 2 §4.
  - Every `Step` variant is matched explicitly, `init_steps` included.
  - In the Read stage, a failing step leaves a warning instead of an error.
  - Each entry's `CommandMetadata` is built in code next to its builder.
- Depends on: steps 0 and 2. Can run in parallel with step 4.
- Proof:
  - `cargo test -p liquers-core --lib store::realm` runs the `realm.rs` tests except the storage
    model half of `registered_entry_is_named_like_a_builtin`, which comes in step 6.
  - `cargo test -p liquers-lib --test registry_export`.
  - `liquers-validate -- '-store_config/defaults/media_type-fill/file_facts'` exits 0.
- Rollback: revert, then regenerate the registry.

### Step 6: storage models
- Files / symbols: new `store/storage.rs`: `MetadataStorage`, `NativeStorage`, `PerEntryStorage`,
  `Layout`, `GroupedStorage`, `NoStorage`, `MemoryCentralStorage` (test-only). Builders are
  registered in `register_builtin_store_config`, and in `register_test_store_config` for
  `memory_central`.
- Change: Phase 2 §3. The group file is a JSON `BTreeMap<String, MetadataRecord>`, so its bytes are
  deterministic.
- Depends on: steps 3 and 5.
- Proof: `cargo test -p liquers-core --lib store::storage`; regenerate the registry
  (`registry_export`).
- Rollback: revert the module and its registrations.

### Step 7: write strategies
- Files / symbols: new `store/writes.rs`: `WriteStrategy`, `WriteSet`, `ObjectWrite`, `WriteTarget`,
  `PreparedWrite`, `Recovery`, `DirectWrite`, `AtomicWrite`, `JournalWrite`, and their builders.
- Change: Phase 2 §5. The journal is a JSON list of objects, each with its staged key.
- Depends on: step 6, which the transactional tests run over.
- Proof:
  - `cargo test -p liquers-core --features store-conformance --lib store::writes` runs the
    `writes.rs` tests except `each_strategy_follows_its_protocol`, which needs the pipeline and
    moves to step 8.
  - Regenerate the registry.
- Rollback: revert.

### Step 8: `MetadataPipelineStore`
- Files / symbols: new `store/pipeline.rs`: `MetadataPipelineStore`, `PipelineParts` (with
  `read_only`), `MetadataCache`, `GroupLocks`, central storage with backups.
- Change: the Phase 2 §6 flows.
  - The pipeline reports no data access.
  - Locks are taken through `scc::HashMap::entry`, and evicted with `remove_if(|l|
    Arc::strong_count(l) == 1)` after release, so taking and evicting a lock cannot race.
- Depends on: steps 4–7.
- Proof: `cargo test -p liquers-core --features store-conformance --lib store::pipeline` and
  `each_strategy_follows_its_protocol`.
- Rollback: revert. Nothing builds a pipeline until step 10.

### Step 9: the asset manager reads `version_verification`
- Files / symbols: `liquers-core/src/assets.rs` `decide_external_change` and its callers.
- Change: `store.version_verification(&key)` overrides `AssetManagerOptions` when it is `Some`.
  The pipeline answers `Some(OnRead)` for a mount whose read chain contains `content_hash`.
- Depends on: step 8.
- Proof: the two `assets.rs` tests;
  `cargo test -p liquers-core --test external_change_integration` unchanged.
- Rollback: revert. Every store answers `None` again.

### Step 10: configuration types, defaults, `build_store`
- Files / symbols:
  - `liquers-core/src/store_config.rs`: `StoreConfig` and `StoreRouterConfig` gain `read_only` and
    `metadata`; new `MetadataConfig` and `METADATA_DEFAULTS`.
  - `store_factory.rs`: `StoreRouterBuilder::with_store_config(CommandMetadataRegistry,
    StoreConfigFunctions)`, `build_store` and `build_store_without_env_expansion`. The existing
    `build` and `build_without_env_expansion` wrap mounts and refuse router-level sections.
  - `environment_builder.rs` `build` uses its own command registry.
  - New `liquers-core/tests/store_adapters.rs`.
- Change: Phase 2 §7, §Configuration and Integration Points. A non-empty old `metadata` map is
  refused.
- Depends on: step 9.
- Proof:
  - `cargo test -p liquers-core --features store-conformance --test store_adapters`, except the
    front-matter stand-in test, which comes with step 11.
  - `cargo test -p liquers-store`, since `create_router*` changes.
  - `cargo test -p liquers-lib --lib --tests`.
  - `bash scripts/check-build-matrix.sh`.
- Rollback: revert. Configurations without the new fields are unaffected.

### Step 11: conformance suites `A1`–`A7`
- Files / symbols: `liquers-core/tests/store_conformance_CONF.rs` and its `status_table`
  generator; the conformance table in `STORE_IMPLEMENTATION_GUIDE.md` §9.
- Change: the Phase 3 fixtures, each over `SharedStore`. A known divergence is an `AllowedFailure`
  naming its issue.
- Depends on: step 10.
- Proof: `cargo test -p liquers-core --features store-conformance --test store_conformance_CONF`;
  `front_matter_write_back_through_the_pipeline`.
- Rollback: remove the suites.

### Step 12: `front_matter` and `front_matter_sync` in `liquers-lib`
- Files / symbols:
  - New `liquers-lib/src/store_config.rs`: the handlers and `register_lib_store_config`.
  - Callers: `liquers-lib/src/commands.rs` `register_commands`, the exporter's `build_registry`
    and `full_registry`.
  - The overviews use `executable_commands()`: `commands_doc` in `liquers-lib/src/commands.rs`, and
    `liquers-lib/src/search/command_catalog.rs`.
  - `liquers-records`: a public front-matter block parser, if `parse_structure` does not already
    expose one.
- Change: the catalogue rows, behind feature `records`.
- Depends on: step 10.
- Proof:
  - `cargo test -p liquers-lib --lib store_config`, plus `commands_doc_omits_the_store_config_realm` and
    `command_catalog_omits_the_store_config_realm`.
  - `cargo test -p liquers-lib --test registry_export`, after regenerating the registry.
  - `bash scripts/check-build-matrix.sh`.
  - Re-validate every Phase 3 spec against the exported registry (Examples 1 and 2, and the §7
    defaults).
- Rollback: revert, then regenerate.

### Step 13: OpenDAL data-only access
- Files / symbols: `liquers-store/src/opendal_store.rs`;
  `liquers-store/tests/store_conformance_CONF.rs`.
- Change:
  - The data-only methods over `opendal::Operator`.
  - `rename` only where the service has it.
  - `data_facts` comes from `stat`, with the service's ETag.
- Depends on: step 10. Can run in parallel with steps 14 and 15.
- Proof: `cargo test -p liquers-store --test store_conformance_CONF` runs `a3_opendal_fs_per_entry`.
- Rollback: revert. The trait defaults refuse again.

### Step 14: HTTP 501 test and Python getters
- Files / symbols: `liquers-axum/tests/store_api_integration.rs`; the `MetadataRecord` wrapper in
  `liquers-py/src/`.
- Change: `read_only_store_write_is_501`; getters for `created` and `etag`.
- Depends on: step 10.
- Proof: `cargo test -p liquers-axum --test store_api_integration`; `cargo check -p liquers-py`.
- Rollback: revert.

### Step 15: `liquers-web`: `isReadOnly`, the optional data-only methods, declarations
- Files / symbols:
  - `liquers-web/src/store/`: `JsStore` and `LiquersStore`.
  - `liquers-web/src/typescript.rs`: `LiquersStoreEntry` gains `read_only` and `metadata`.
  - `liquers-web/src/environment.rs`: `commandNames` uses `executable_commands()`.
  - The stubs and `liquers-web/README.md`.
- Change: `JsStore` looks the optional methods up once, when it adapts the object, and reports
  `data_capabilities` from what it found. The built-in browser stores keep refusing data access.
- Depends on: step 10.
- Proof (after `cargo clean`, per `CLAUDE.md`):
  - the four `store_js_STORE.rs` tests;
  - `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`;
  - `./liquers-web/scripts/check-stubs.sh`.
- Rollback: revert. If the step cannot be finished, it becomes an issue (Phase 1 readiness item 5).

## Testing Plan

Each step runs its own proof, then the default loop (`cargo test -p liquers-lib --lib --tests`), so
a breakage stops at its step.

- **Feature matrix:** `bash scripts/check-build-matrix.sh` runs after steps 2, 10 and 12. Those
  touch trait bounds, `cfg` features and optional dependencies.
- **Final check, after step 15:**
  - `liquers-core` with `--features store-conformance`;
  - `liquers-store`, `liquers-lib`, `liquers-axum`;
  - the `liquers-web` Node loop.

  Each runs separately, with `cargo clean` between runs if disk requires it.
- **Known flake:** `volatile_dependency_has_one_value_per_evaluation`
  (`EVALUATION-SESSION-VOLATILE-TEST-INTERMITTENT`) is re-run once if it fails.

## Rollback Plan

Every step is one commit and reverts on its own; the registry is regenerated after reverting
step 5, 6, 7 or 12. Until step 10, nothing is reachable from configuration. If the work stops
partway:
- the landed steps stay, tested;
- the remainder becomes an issue (§5.6), and Phase 5 records it.

## Documentation Updates

Create:
- `specs/reference/REALMS.md`.
- `specs/guides/STORE_CONFIG_GUIDE.md`, including the `/` escaping and the empty-list meanings.

Update:
- `STORE_SEMANTICS.md`.
- `STORE_CONFIG_FSD.md`, with the defaults table.
- `STORE_IMPLEMENTATION_GUIDE.md`, with the conformance table.
- `STORE_FACTORY_GUIDE.md`.
- `ASSETS.md`.
- `PROJECT_OVERVIEW.md`. Its realm section now holds true, and points to `REALMS.md`.
- `LANGUAGE-INTEGRATION_GUIDE.md`, following the Phase 2 plan, with `STORE14`–`STORE18`.
- `CLAUDE.md`.
- `liquers-web/README.md`.
- `.claude/skills/liquers-validate/SKILL.md`.

Each updated `reference/` or `guides/` document gets a `## History` row and a bumped `reviewed:`.

Regenerate and register:
- `specs/command_registry.yaml`, in steps 5–7 and 12.
- `specs/index.csv`.
- `specs/README.md`.
- `affects_docs` gains the two new documents when they exist.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
