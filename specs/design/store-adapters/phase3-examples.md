# Phase 3: Examples & Use-cases — store-adapters

All tests are runnable. The only conceptual pieces are the Rust sketches in the two examples, which
show the calls a test makes rather than its setup. Every spec below was parsed with
`liquers-validate --no-registry` in its filled form (`-store_config/…`). Validating against the
registry becomes possible once Phase 4 exports the realm to `specs/command_registry.yaml`.

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A read-only documentation mount | The agent-memory `docs` mount: nothing written, front matter, hash on change | AC-1, AC-2, AC-4, AC-5, AC-8 |
| 2 | Example | A writable memory store | Grouped records, transactional writes, validation, recovery | AC-11, AC-14, AC-19, AC-20 |
| 3 | Tests | `liquers-core` `store/` unit tests | Compiler, decorator, models, strategies, pipeline | AC-1–AC-4, AC-9, AC-11, AC-13–AC-22 |
| 4 | Tests | `liquers-core/tests/store_adapters.rs` | Configuration through `StoreRouterBuilder` and the router | AC-6, AC-8, AC-9, AC-10, AC-12, AC-17, AC-21, AC-22 |
| 5 | Tests | `store_conformance_CONF.rs` `A1`–`A7` (core and `liquers-store`) | Adapted stores pass the suite | AC-6, AC-7 |
| 6 | Tests | `liquers-lib/src/store_config.rs` | `front_matter`, `front_matter_sync` | AC-5, AC-10 |
| 7 | Tests | `liquers-axum/tests/store_api_integration.rs`, `liquers-web/tests/store_js_STORE.rs` | 501 over HTTP; a JavaScript store under configuration | AC-1, AC-9, AC-11, AC-15 |

## Example 1: A read-only documentation mount

Agent memory (`agent-memory-mvp` decisions 2 and 4) mounts a git checkout of Markdown notes. It must
never write into the checkout, and it must answer `title` and `description` from the front matter.

```yaml
stores:
  - type: filesystem
    prefix: docs
    config: { path: ./docs }
    read_only: true
    metadata:
      storage: none
      on_read: media_type/file_facts/content_hash-on_change/front_matter
```

Fields left out take their defaults (Phase 2 §7): `writes: direct`, `persist_defaults: false`,
`on_default: defaults/media_type-fill/file_facts`, `on_validate: ""`, and the default `on_write`,
which is never reached because the mount is read-only. The builder builds `AsyncFileStore`, wraps it
in `MetadataPipelineStore` (storage `none`, built read-only so it never writes), then in
`ReadOnlyStore`, and puts it in the router. The pipeline reports no data access, so `ReadOnlyStore`
forwards reads to it and the chains run.

```rust
let (cmr, functions) = lib_store_config();       // built-ins plus liquers-lib's front_matter
let store = StoreRouterBuilder::from_yaml(YAML, Box::new(core_store_factory()))?
    .with_store_config(cmr, functions)
    .build_store()?;
let key = parse_key("docs/notes/a.md")?;
let info = store.get_asset_info(&key).await?;
assert_eq!(info.title, "Store adapters");          // from the front matter
assert_eq!(info.media_type, "text/markdown");
let record = store.get_metadata(&key).await?.metadata_record().expect("record");
assert!(record.version.is_some());                 // first read: no previous record, so hashed
let err = store.set(&parse_key("docs/notes/b.md")?, b"x", &meta).await.unwrap_err();
assert_eq!(err.error_type, ErrorType::NotSupported);
assert!(store.is_read_only());
// The checkout holds exactly the files it held before: no sidecar, no __metadata__ folder.
```

Steps for a read of `docs/notes/a.md`:
1. The router selects the mount.
2. `ReadOnlyStore` forwards the read.
3. The pipeline calls `writes.recover`, which has nothing to recover.
4. It asks the storage model (`none`) for a record, gets nothing, and runs the default chain.
5. It runs the read chain. `content_hash` and `front_matter` need the bytes, so the pipeline reads
   them first.
6. The record is cached, keyed by the `DataFacts`.

A second read with unchanged facts is a cache hit and reads no bytes. After `touch a.md` the facts
differ, so `content_hash-on_change` hashes again, and the version changes only if the content did.

## Example 2: A writable memory store

The agent memory store keeps one record per folder and must never show a half-written entry.

```yaml
  - type: filesystem
    prefix: memory
    config: { path: ./memory }
    metadata:
      storage: grouped-__metadata__.json
      writes: transactional
      on_validate: require-title/media_type
      on_write: defaults/media_type-fill/file_facts/content_hash/updated
```

What this adds beyond Example 1:
- **Validation.** `set("memory/a/x.md", data, m)` with an empty `title` fails, naming `title`, and
  nothing is written. A `media_type` of `text/plain` on `x.md` fails the same way (AC-14).
- **Grouped records.** A successful write leaves `memory/a/x.md` and one `memory/a/__metadata__.json`
  holding the record of every entry in `a/`. The write commits under the group lock: staged
  objects, then the journal `x.md.__txn__`, then the moves.
- **Recovery.** The test kills the commit after the journal is written. The next
  `get_metadata("memory/a/x.md")` rolls the write forward, removes the staged objects and returns
  the new record with a warning in its log. `sync()` on the store reports the same repair (AC-20).

## Edge and Error Cases

| Symptom | Expected | Test |
|---|---|---|
| `storage: transactional` (a write strategy in the storage field) | build fails, naming mount and field `storage` | `compile_refuses_component_of_the_wrong_kind` |
| `storage: per_entry/grouped` (two steps) | build fails: a component spec is one action | `component_spec_with_two_steps_is_refused` |
| `content_hash-sometimes` (invalid option) | build fails, naming the parameter | `compile_refuses_an_invalid_option` |
| A link or placeholder parameter | build fails: literals only | `compile_refuses_link_parameters` |
| `on_read: -R-key/docs/notes/-store_config/front_matter` | build fails: resource steps are refused for now | `compile_refuses_set_cwd_and_resource_steps` |
| `on_read: -lui/front_matter` (header naming another realm) | build fails, naming the realm | `fill_realm_refuses_another_realm` |
| `on_read: -/front_matter` (header with an empty name) | the name is filled in, as for no header | `fill_realm_fills_an_empty_header_name` |
| `require` in `on_read`; `require` with no fields | build fails | `compile_refuses_entry_in_a_stage_it_does_not_allow`, `require_without_fields_is_refused` |
| `metadata` on a store without data-only access (`NoAsyncStore`; a JavaScript object without `getData`) | build fails, naming the mount and the capability | `metadata_on_store_without_data_access_is_refused`, `store15_metadata_without_data_methods_refused` |
| `writes: atomic` on a backend with neither rename nor atomic write | build fails (AC-17) | `atomic_refused_without_rename_or_atomic_write` |
| A non-empty old free-form `metadata` map | build fails, naming the field | `old_metadata_map_is_refused` |
| A member section naming handlers under central storage | build fails, naming the member | `central_member_with_handlers_is_refused` |
| A router-level section given to `build()` (bare router) | build fails, naming `build_store` | `bare_router_build_refuses_router_sections` |
| Evaluating a `store_config` query through the asset manager | `not_supported` naming the realm | `executor_refuses_the_store_config_realm` |
| `read_only` over a store without data access | built, with a warning that reads may write | `read_only_over_store_without_data_access_warns` |
| A key named `__metadata__.json` or `x.md.__txn__` written by a client | `key_not_supported`, and `is_supported` agrees | `grouped_reserves_file_name_in_every_folder`, `transactional_reserved_names_refused` |
| Same size, preserved timestamp (`cp -p`) | `on_change` misses it; `always` and `verify_stored_versions` catch it | `content_hash_on_change_misses_cp_p_rewrite`, `verify_stored_versions_catches_cp_p_rewrite` |
| A `LegacyMetadata` record in storage | returned untouched, with a warning; no chain runs | `legacy_record_bypasses_chains` |
| Two concurrent writes in one folder of a grouped store | both records present afterwards | `grouped_sibling_writes_are_serialized` |
| `metadata: {}` | not wrapped, the same as no section | `empty_metadata_section_is_no_section` |
| A record carrying `created` read by an older build | the older build falls back to `LegacyMetadata`; nothing is lost | `new_record_adds_only_created_and_etag`, `unknown_field_falls_back_to_legacy_losslessly` |

## Test Plan

**Shared helpers** (new; `liquers-core/src/store/test_support.rs`, under `cfg(any(test, feature =
"store-conformance"))`, public with the feature so integration tests can use them):
- `RecordingStore`, which logs every call;
- `FailingStore`, which fails its *n*-th data-only call;
- `SharedStore(Arc<dyn AsyncStore>)`, a public forwarder so a built `Arc` store can enter
  `GenericFixture`.

**`liquers-core/src/plan.rs`** (step 0):
- `realm_header_applies_to_every_action` (AC-21): `-r/a/b` plans both actions in realm `r`.
- `realm_header_applies_to_a_single_action` (AC-21).
- `realm_header_applies_after_a_resource_segment` (AC-21).
- `default_realm_plans_unchanged` (AC-6).

**`liquers-core/src/metadata.rs`:**
- `backend_time_round_trips_at_its_precision` (AC-4).
- `new_record_adds_only_created_and_etag` (AC-6): against a committed fixture of the pre-change
  field list.
- `unknown_field_falls_back_to_legacy_losslessly` (AC-6).

**`liquers-core/src/store.rs`:**
- `data_only_get_does_not_create_sidecar` (AC-1).
- `data_facts_change_when_file_is_rewritten` (AC-4).
- `router_forwards_data_only_methods` (AC-8).
- `router_is_read_only_when_every_member_is` (AC-1, AC-8).
- `router_data_capabilities_are_the_intersection` (AC-8).

**`liquers-core/src/store/read_only.rs`:**
- `read_only_refuses_every_write_with_not_supported` (AC-1): `set`, `set_metadata`, `remove`,
  `makedir`, `removedir` and the data-only writes.
- `read_only_file_store_creates_nothing` (AC-1): a temporary directory is listed before and after
  `listdir`, `listdir_asset_info`, `get`, `get_metadata` and `get_asset_info` of entries that have
  no sidecar, and the two listings are identical.
- `read_only_reports_is_read_only` (AC-1).

**`liquers-core/src/store/realm.rs`:**
- `fill_realm_adds_header_to_headerless_spec` (AC-21).
- `fill_realm_keeps_explicit_header` (AC-21).
- `fill_realm_fills_an_empty_header_name` (AC-21).
- `fill_realm_refuses_another_realm` (AC-9).
- `compile_refuses_unknown_entry_with_position` (AC-9).
- `compile_refuses_an_invalid_option` (AC-9).
- `compile_refuses_link_parameters` (AC-9).
- `compile_refuses_entry_in_a_stage_it_does_not_allow` (AC-9).
- `require_without_fields_is_refused` (AC-9).
- `compile_refuses_component_of_the_wrong_kind` (AC-9).
- `component_spec_with_two_steps_is_refused` (AC-9).
- `compile_refuses_set_cwd_and_resource_steps` (AC-9).
- `chain_runs_in_order_and_the_later_step_wins` (AC-16): `set-title-A/set-title-B` gives `B`, and
  reversed it gives `A`. Run in the Write and the Read stage.
- `second_step_receives_the_first_steps_output` (AC-21): `media_type` then `require-media_type`.
  The second step sees the field the first set.
- `fill_mode_keeps_a_present_value` (AC-15).
- `planner_messages_are_logged_once` (AC-21): two runs over one record add one entry.
- `read_stage_error_becomes_a_warning` (AC-15).
- `validate_stage_error_aborts` (AC-14).
- `needs_bytes_only_when_a_step_needs_them` (AC-4).
- `content_hash_on_change_follows_size_time_and_etag` (AC-4).
- `content_hash_on_change_misses_cp_p_rewrite` (AC-4).
- `defaults_matches_the_documented_record` (AC-22): today's fields, without `updated` or the
  warnings.
- `metadata_config_defaults_match_the_documented_table` (AC-22).
- `set_field_replaces_its_default_whole` (AC-22): `on_read: file_facts` drops the default
  `media_type-fill`.
- `empty_spec_means_no_steps` (AC-22).
- `registered_entry_is_named_like_a_builtin` (AC-10, AC-21): a handler and a storage model
  registered by a test.
- `executor_refuses_the_store_config_realm` (AC-9).

**`liquers-core/src/store/storage.rs`:**
- `per_entry_layouts_write_under_their_template` (AC-3).
- `per_entry_reserved_names_refused_by_is_supported_and_set` (AC-3).
- `grouped_change_keeps_sibling_records` (AC-11).
- `grouped_remove_drops_only_its_record` (AC-11).
- `grouped_listing_reads_the_group_file_once` (AC-11): `RecordingStore` counts one `get_data` of
  the group file.
- `grouped_reserves_file_name_in_every_folder` (AC-11).

**`liquers-core/src/store/writes.rs`** (each strategy over `FailingStore`, for every *n* up to the
number of calls a commit makes; written with step 8):
- `each_strategy_follows_its_protocol` (AC-17): `RecordingStore` shows in-place writes for
  `direct`, stage then rename for `atomic`, and stage, journal, move for `transactional`, for each
  of `set`, `set_metadata` and `remove`.
- `atomic_failure_at_each_step_leaves_old_or_new` (AC-18).
- `atomic_staging_key_is_never_listed` (AC-18).
- `atomic_refused_without_rename_or_atomic_write` (AC-17).
- `transactional_interrupted_at_each_step_recovers_consistently` (AC-19). Covers `set` and
  `remove`, for `per_entry`, `grouped` (the other records of the group unchanged) and
  `memory_central` with a backup target.
- `transactional_reserved_names_refused` (AC-19).
- `sync_reports_repairs` (AC-20).

**`liquers-core/src/store/pipeline.rs`:**
- `recovery_on_read_and_listing_completes_the_write` (AC-20). Both the roll-forward and the
  roll-back branch. Staged objects are removed, and a warning is in the log.
- `storage_none_never_writes` (AC-2).
- `persist_defaults_true_stores_the_default_once` (AC-13).
- `persist_defaults_false_writes_nothing_and_is_stable` (AC-2, AC-13).
- `write_chain_overwrites_and_fills` (AC-15).
- `read_chain_fills_a_file_written_outside` (AC-15).
- `validator_rejects_and_nothing_is_written` (AC-14).
- `media_type_in_write_chain_corrects_the_field` (AC-14).
- `created_comes_from_backend_facts_after_commit` (AC-15).
- `listing_rehashes_after_an_outside_edit` (AC-4): under `always` and under `on_change`.
- `listing_reads_no_bytes_for_unchanged_entries` (AC-4).
- `grouped_sibling_writes_are_serialized` (AC-11).
- `legacy_record_bypasses_chains` (AC-15).
- `read_only_pipeline_never_persists` (AC-1).

**`liquers-core/src/assets.rs`:**
- `mount_content_hash_overrides_version_verification` (AC-4).
- `verify_stored_versions_catches_cp_p_rewrite` (AC-4).

**Integration, `liquers-core/tests/store_adapters.rs`** (`#![cfg(feature = "store-conformance")]`):
- `config_without_metadata_section_is_not_wrapped` (AC-6).
- `empty_metadata_section_is_no_section` (AC-6, AC-22).
- `old_metadata_map_is_refused` (AC-9).
- `read_only_mount_from_config` (AC-1, AC-8).
- `read_only_over_store_without_data_access_warns` (AC-1).
- `unknown_storage_model_names_mount_and_field` (AC-9).
- `metadata_on_store_without_data_access_is_refused` (AC-9).
- `bare_router_build_refuses_router_sections` (AC-9).
- `adapted_member_behaves_the_same_through_the_router` (AC-8): `get_asset_info` and listings
  included.
- `central_storage_answers_reads_and_writes` (AC-12): the members' data is byte-identical to a run
  without central storage.
- `central_member_without_section_stores_nothing` (AC-12).
- `central_backup_written_with_every_central_write` (AC-12).
- `central_backup_restores_a_missing_record` (AC-12): copied to central, with a warning.
- `central_member_with_handlers_is_refused` (AC-9).
- `front_matter_write_back_through_the_pipeline` (AC-5): after `set_metadata`, the file's front
  matter and `get_metadata` agree. Runs with a test handler that stands in for the lib one, since
  core cannot depend on `liquers-lib`.
- `same_section_same_behaviour` (AC-22).

**Conformance, `liquers-core/tests/store_conformance_CONF.rs`** (AC-7). New suites are `A1`–`A7`,
named apart from `C1`–`C6` and from `liquers-store`'s `c6`/`c7`:
- `a1_read_only_file_store`: write and remove false.
- `a2_storage_none`: stored_metadata false.
- `a3_per_entry_each_layout`: four suites.
- `a4_grouped`.
- `a5_memory_central`.
- `a6_each_write_strategy`: three suites.
- `a7_front_matter`: the test stand-in handler, in a read chain.

`C1`–`C6` stay unchanged (AC-6). Rows go into the `status_table` generator's output in
`STORE_IMPLEMENTATION_GUIDE.md` §9.

**`liquers-store/tests/store_conformance_CONF.rs`:**
- `a3_opendal_fs_per_entry` (AC-1, AC-7): OpenDAL with data-only access, under `per_entry` and
  read-only.

**`liquers-lib/src/store_config.rs`** (`#[cfg(feature = "records")]`):
- `front_matter_projects_title_and_description` (AC-5).
- `front_matter_sync_rewrites_the_description` (AC-5).
- `markdown_without_front_matter_reads_without_error` (AC-5).
- `lib_registration_makes_front_matter_nameable` (AC-10).

**Bindings:**
- `liquers-axum/tests/store_api_integration.rs` `read_only_store_write_is_501` (AC-1).
- `liquers-web/tests/store_js_STORE.rs`. These are guide tests `STORE14`–`STORE17`, added to the
  guide in Phase 5. `STORE14` extends `STORE09` from routing to configuration. `STORE18` is `NA`,
  linking `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE`.
  - `store14_read_only_js_store_refuses_writes` (AC-1);
  - `store15_metadata_without_data_methods_refused` (AC-9);
  - `store16_js_store_under_grouped_storage_round_trips` (AC-11);
  - `store17_created_round_trips_from_js_time` (AC-15).

**Commands to run:**

```bash
cargo test -p liquers-core --lib -- plan:: metadata:: store
cargo test -p liquers-core --features store-conformance --lib store::
cargo test -p liquers-core --features store-conformance --test store_adapters --test store_conformance_CONF
cargo test -p liquers-store --test store_conformance_CONF
cargo test -p liquers-lib --lib store_config
cargo test -p liquers-lib --test registry_export
cargo test -p liquers-axum --test store_api_integration
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test store_js_STORE
bash scripts/check-build-matrix.sh
```

## Learning Log

- A spec that parses is not a spec that means what you wrote: `-R/x/to_text` and `-R/x/-/to_text`
  both validate. Store specs avoid the trap because the compiler refuses every resource step, so a
  misplaced `-R` fails at build.
- A query parameter cannot hold a `/`, which is why `per_entry` takes a layout *name* rather than a
  template. This should go into `STORE_CONFIG_GUIDE.md`.
- The realm-filled form is the one to validate. Without the header the tool plans the actions in
  realm `""`, where the entries do not exist.
