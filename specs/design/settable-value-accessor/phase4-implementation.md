# Phase 4: Implementation Plan — settable-value-accessor

## Overview

Bottom up, so each step compiles and is tested before the next one uses it: the core contract and
hub (with memory and test roots), then the asset and store roots, then the query and document
steps, then the value hooks; then lib (variant, commands, records steps), axum, and the two
language wrappers, which are independent of each other once lib is done. No issue has to be fixed
first (Phase 2 preflight). Native work uses `cargo test -p liquers-core` and `-p liquers-lib`
only; the wasm, Python and axum loops run at their own steps, each after `cargo clean` when disk is
short (`CLAUDE.md` §Building and testing).

## Progress
- [ ] Step 1: Core contract types and the conflict error
- [ ] Step 2: The hub, the memory root and the wrappers
- [ ] Step 3: The store, keyed-asset and asset roots
- [ ] Step 4: Document, recipe, query, action and argument steps
- [ ] Step 5: `ValueInterface` accessor hooks
- [ ] Step 6: `ExtValue::Accessor` in lib and in liquers-web's matches
- [ ] Step 7: The `acc` commands and the registry
- [ ] Step 8: Record steps
- [ ] Step 9: The `/api/acc` routes and socket
- [ ] Step 10: The Python wrapper
- [ ] Step 11: The JavaScript wrapper
- [ ] Step 12: Whole-change checks

## Implementation Steps

### Step 1: Core contract types and the conflict error
- Files / symbols: new `liquers-core/src/accessor/mod.rs` (`Fingerprint`, `AccessorSnapshot`,
  `AccessorEvent`, `AccessorInfo`, `ExpiryPolicy`, `Capabilities`, `SetOptions`,
  `PartDescription`, `ValueAccessor`, `ProjectionStep`, `InvalidationSink`, `AccessorRoot`,
  `RootEvent`, `AccessorBox`, `ErasedAccessor`); `lib.rs` (`pub mod accessor`); `error.rs`
  `Error::accessor_conflict(address: &Query, current: Option<&Fingerprint>)`.
- Change: the Phase 2 signatures; `Fingerprint::token` is the anchor and content as 16 + 32 hex
  digits joined by `-`, and `from_token` parses it back.
- Depends on: none.
- Proof: `cargo test -p liquers-core accessor --lib` (`fingerprint_token_round_trips`).
- Rollback: delete the module and the constructor.

### Step 2: The hub, the memory root and the wrappers
- Files / symbols: `accessor/hub.rs` (`AccessorHub`, its `Node` tree, `driver()`), `accessor/root.rs`
  (`MemoryRoot`, `memory_accessor`), `accessor/wrap.rs` (`read_only`, `validated`).
- Change: the write lock (`tokio::sync::Mutex`), the tree lock (`std::sync::Mutex`, never held
  across `.await`), derive-and-hash per subscribed node, conflict check per Phase 2, sinks and
  senders fired after unlocking. Native spawns the watcher only when
  `tokio::runtime::Handle::try_current()` succeeds; otherwise it falls back to pull, as on wasm.
- Depends on: Step 1.
- Proof: the eight `hub.rs` unit tests of Phase 3 (AC-4, AC-5, AC-14, AC-15, AC-18, AC-19, AC-22).
  Step 2 uses a test `FieldStep` over `Value::Object`; Step 4 replaces it with the real one.
- Rollback: as Step 1.

### Step 3: The store, keyed-asset and asset roots
- Files / symbols: `accessor/root.rs` `StoreRoot`, `StoreView`, `store_accessor`, `KeyRoot`,
  `key_accessor`, `AssetRefRoot`, `asset_accessor`.
- Change: Phase 2's read, write, info, log and notification rules; `KeyRoot` re-subscribes after a
  replacement. Uses `AssetManager::{get, set_state, set_binary, get_asset_info, can_make}`,
  `AsyncStore::{get, get_metadata, contains}`, `AssetRef::{subscribe_to_notifications,
  get_metadata}`.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core --test value_accessor` for `outside_writer_is_detected`,
  `expiry_and_removal_are_reported`, `keyed_asset_set_overrides_sources_or_creates`,
  `expired_asset_get_re_evaluates`, `store_views_and_a_missing_key`,
  `progress_and_log_are_relayed`.
- Rollback: remove the three roots; Step 2 stands alone.

### Step 4: Document, recipe, query, action and argument steps
- Files / symbols: `accessor/steps.rs` `FieldStep`, `ItemStep`, `RecipeStep`, `QueryFieldStep`,
  `PipelineFieldStep`, `ActionStep`, `ArgumentStep`.
- Change: documents parse from `Value::Text` / `Value::Bytes` (YAML or JSON, chosen by the
  parent's `data_format` or by content) and from `Value::Object` / `Value::Array`, and write back
  in the parent's representation. The action part is a `Value::Pipeline` of one element; the
  argument part is the argument's JSON as a `Value`. Steps hold the registry from
  `command_metadata_snapshot`.
- Depends on: Steps 2 and 3.
- Proof: `recipe_argument_projection_reads_part_and_hints`, `recipe_argument_set_writes_through`,
  `refused_argument_set_writes_nothing`, `query_steps_work_on_any_query_root`.
- Rollback: remove the steps.

### Step 5: `ValueInterface` accessor hooks
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::{from_accessor,
  try_into_accessor}` (defaulted); core `Value` keeps the defaults; the module doc gains the
  handle-value section (Phase 2 API docs).
- Depends on: Step 1. Can run in parallel with Steps 2–4.
- Proof: `cargo test -p liquers-core --lib`; `python3 scripts/check_api_docs.py liquers-core/src/value.rs`.
- Rollback: remove the two defaulted methods.

### Step 6: `ExtValue::Accessor` in lib and in liquers-web's matches
- Files / symbols: `liquers-lib/src/value/mod.rs` (variant, `TypeInfo::new("accessor")` with no
  formats, every `ExtValue` match, `data_node` opaque), `value/extended.rs`
  (`ValueExtension::{erased_accessor, from_erased_accessor}` and `CombinedValue`'s forwarding),
  `liquers-web/src/default_value.rs` (arms returning `None` until Step 11).
- Depends on: Step 5.
- Proof: `ext_value_accessor_hooks_round_trip`, `accessor_value_is_never_stored`;
  `cargo test -p liquers-lib --test value_type_system`.
- Rollback: remove the variant; every match arm goes with it.

### Step 7: The `acc` commands and the registry
- Files / symbols: new `liquers-lib/src/accessor/mod.rs` (`register_commands`; `accessor`,
  `store`, `recipe`, `query`, `pipeline`, `action`, `argument`, `field`, `item`, `get`), wired
  where `mem/` and `markdown/` register; `specs/command_registry.yaml` regenerated with a
  CHANGELOG line.
- Depends on: Steps 4 and 6.
- Proof: `accessor_query_projects_recipe_argument`, `accessor_address_resolves_in_a_second_environment`,
  `document_field_projection`, `store_command_views`; `cargo test -p liquers-lib --test registry_export`.
- Rollback: unregister the namespace and regenerate the registry.

### Step 8: Record steps
- Files / symbols: `liquers-lib/src/records/accessor.rs` `RowStep`, `ColumnStep`, `CellStep`
  (`#[cfg(feature = "records")]`).
- Depends on: Step 6. Can run in parallel with Step 7.
- Proof: `record_cell_projection`; `cargo test -p liquers-lib --no-default-features --features records --lib --tests`.
- Rollback: remove the file.

### Step 9: The `/api/acc` routes and socket
- Files / symbols: new `liquers-axum/src/accessor/` (`AccessorApiBuilder<E>`, handlers, socket
  reusing `assets/websocket.rs`), `api_core/error.rs` (`412`, `428`, `405` for accessor routes),
  the router assembly where `AssetsApiBuilder` is mounted.
- Depends on: Step 7.
- Proof: `cargo test -p liquers-axum --test accessor_api` (needs `libssl-dev`, `CLAUDE.md`).
- Rollback: unmount the builder.

### Step 10: The Python wrapper
- Files / symbols: new `liquers-py/src/accessor.rs`, `lib.rs`, `value.rs`, `environment.rs`;
  `liquers-py/python/liquers/accessors.py` and `.pyi`; `liquers-py/README.md` conformance rows;
  `tests/test_ACCESSOR_use.py`.
- Depends on: Step 7. Can run in parallel with Steps 9 and 11.
- Proof: `bash scripts/test-liquers-py.sh` (`test_ACCESSOR01`–`07`, `test_STUBS_declarations.py`).
- Rollback: remove the module and its registration.

### Step 11: The JavaScript wrapper
- Files / symbols: new `liquers-web/src/accessor.rs`, `default_value.rs` arms, `environment.rs`
  (`accessor`, `memoryAccessor`), `typescript.rs`; `tests/accessor_ACCESSOR.rs`.
- Depends on: Step 7. Can run in parallel with Steps 9 and 10.
- Proof: after `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test accessor_ACCESSOR`
  and `./liquers-web/scripts/check-stubs.sh --build`.
- Rollback: remove the class; the `default_value.rs` arms return `None` again.

### Step 12: Whole-change checks
- Proof: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
  `bash scripts/check-build-matrix.sh`, `python3 scripts/check_api_docs.py` on every touched module.

## Testing Plan

Each step runs only its own tests while it is being written, then once its crate's default loop.
Step 12 runs the core and lib loops and the feature matrix once, because Steps 6 and 8 touch an
`ExtValue` match and a `cfg(feature)`. The axum, Python and wasm loops run once each, at Steps 9,
10 and 11, and again only if a later step changes their crate.

## Rollback Plan

The change is additive: no existing signature changes except two defaulted `ValueInterface`
methods and one `ExtValue` variant. Reverting the PR removes it whole. If only some steps land, the
unticked steps become an issue (§5.6); Steps 1–7 are a usable Rust feature on their own, and 8–11
are independent extensions of it.

## Documentation Updates

Per Phase 2's documentation architecture, in Phase 5: create `specs/reference/VALUE_ACCESSORS.md`
and `specs/guides/VALUE_ACCESSOR_GUIDE.md`; add the `ACCESSOR` feature (§4 table, §5 section,
§6 Python and Browser JavaScript entries, Appendix A tests) to `LANGUAGE-INTEGRATION_GUIDE.md`;
update `VALUE_TYPE_SYSTEM.md`, `PYTHON_INTEGRATION.md`, `PYTHON_GUIDE.md`, `JAVASCRIPT_GUIDE.md`,
`RECIPES_AND_PLANS.md`, `WEB_API_SPECIFICATION.md`, `UI_INTERFACE_FSD.md`, each with a History row
and a `reviewed:` bump; the values chapters and `e-web-api.md` of the three user guides
(`cargo run -p liquers-docs -- check`). Module docs per `API_DOCUMENTATION_GUIDE.md`, checked with
`scripts/check_api_docs.py`. Regenerated: `specs/command_registry.yaml` (Step 7), `specs/index.csv`
and `specs/index.md` (`python3 scripts/docs_index.py`), `specs/README.md`.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
