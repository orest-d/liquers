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
- [x] Step 1: Core contract types and the conflict error (89959099)
- [x] Step 2: The hub, the memory root and the wrappers (89959099)
- [x] Step 3: The store, keyed-asset and asset roots (see Execution Notes)
- [x] Step 4: Document, recipe, query, action and argument steps
- [x] Step 5: `ValueInterface` accessor hooks
- [x] Step 6: `ExtValue::Accessor` in lib and in liquers-web's matches
- [x] Step 7: The `acc` commands and the registry
- [x] Step 8: Record steps
- [x] Step 9: The `/api/acc` routes and socket
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
- Change: the Phase 2 signatures (`V: ValueInterface` on the traits, `BoxFuture` from
  `maybe_send.rs`); `Fingerprint::token` is the anchor and content as 16 + 32 hex digits joined by
  `-`, and `from_token` parses it back; the conflict message ends `current token: <token>`. The
  module doc of `accessor/mod.rs` is written here, to `API_DOCUMENTATION_GUIDE.md`.
- Depends on: none.
- Proof: `cargo test -p liquers-core accessor --lib` (`fingerprint_token_round_trips` in
  `accessor/mod.rs`); `python3 scripts/check_api_docs.py liquers-core/src/accessor/mod.rs`.
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
  They use test `FieldStep` / `ItemStep` over `Value::Object` / `Value::Array`; Step 4 replaces
  them with the real ones.
- Rollback: delete `hub.rs`, `wrap.rs` and `MemoryRoot` in `root.rs`.

### Step 3: The store, keyed-asset and asset roots
- Files / symbols: `accessor/root.rs` `StoreRoot`, `StoreView`, `store_accessor`, `KeyRoot`,
  `key_accessor`, `AssetRefRoot`, `asset_accessor`.
- Change: Phase 2's read, write, info, log and notification rules: a weak environment, the
  status reset and `State::from_value_and_metadata` on every write, the version from
  `AssetRef::get_metadata`, and re-subscription when `AssetRef::id()` changes. Uses `AssetManager::{get, set_state, set_binary, get_asset_info, can_make}`,
  `AsyncStore::{get, get_metadata, contains}`, `AssetRef::{subscribe_to_notifications,
  get_metadata}`.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-core --test value_accessor` for `outside_writer_is_detected`,
  `expiry_and_removal_are_reported`, `keyed_asset_set_overrides_sources_or_creates`,
  `expired_asset_get_re_evaluates`, `store_views_and_a_missing_key`,
  `store_root_sees_a_direct_write_on_refresh`, `progress_and_log_are_relayed`,
  `accessor_does_not_keep_the_environment_alive`.
- Rollback: remove the three roots; Step 2 stands alone.

### Step 4: Document, recipe, query, action and argument steps
- Files / symbols: `accessor/steps.rs` `FieldStep`, `ItemStep`, `RecipeStep`, `QueryFieldStep`,
  `PipelineFieldStep`, `ActionStep`, `ArgumentStep`.
- Change: documents parse from text or bytes (YAML or JSON, chosen by the value's `identifier()`
  and content) and from objects and arrays, and write back in the parent's representation. A
  recipe goes through `try_into_json_value` and serde, and back with `from_recipe`. The action part
  is a one-element `Value::Pipeline`; the argument part is the argument's JSON as a `Value`. A query
  is written back through `QueryEditor` (lenient). Steps share the hub's registry snapshot
  (`get_envref().command_metadata_snapshot()`). The module doc of `pipeline/editor.rs` gains its
  use by accessors.
- Depends on: Steps 2 and 3.
- Proof: `recipe_argument_projection_reads_part_and_hints`, `recipe_argument_set_writes_through`,
  `refused_argument_set_writes_nothing`, `query_steps_work_on_any_query_root`, `hub_error_cases`
  (with the real steps).
- Rollback: remove the steps.

### Step 5: `ValueInterface` accessor hooks
- Files / symbols: `liquers-core/src/value.rs` `ValueInterface::{from_accessor,
  try_into_accessor}` (defaulted); core `Value` keeps the defaults; the module doc gains the
  handle-value section (Phase 2 API docs).
- Depends on: Step 1. Can run in parallel with Steps 2–4.
- Proof: `cargo test -p liquers-core --lib`; `python3 scripts/check_api_docs.py liquers-core/src/value.rs`.
- Rollback: remove the two defaulted methods.

### Step 6: `ExtValue::Accessor` in lib and in liquers-web's matches
- Files / symbols: `liquers-lib/src/value/mod.rs` (variant, `TypeInfo::new("Accessor")` with no
  formats, every `ExtValue` match, `data_node` opaque, module doc), `value/extended.rs`
  (`ValueExtension::{erased_accessor, from_erased_accessor}` and `CombinedValue`'s forwarding,
  module doc), the other exhaustive matches (`egui/mod.rs`, `ui/web/html.rs`,
  `tests/value_type_system.rs` `samples`), `liquers-web/src/default_value.rs` (arms returning
  `None` until Step 11).
- Depends on: Step 5.
- Proof: `ext_value_accessor_hooks_round_trip`, `accessor_value_is_never_stored`;
  `cargo test -p liquers-lib --test value_type_system`;
  `cargo check -p liquers-lib --no-default-features --features webui`;
  `cargo check -p liquers-web --target wasm32-unknown-unknown`.
- Rollback: remove the variant; every match arm goes with it.

### Step 7: The `acc` commands and the registry
- Files / symbols: new `liquers-lib/src/accessor/mod.rs` (the ten commands and
  `#[macro_export] register_accessor_commands!`, module doc), called from `commands.rs`
  (`register_all_commands!`), `bin/export_command_registry.rs` (`Group::Accessor`),
  `tests/registry_export.rs`, `liquers-py/src/builtins.rs` and `liquers-web/src/builtins.rs`;
  `specs/command_registry.yaml` regenerated with a CHANGELOG line.
- Depends on: Steps 4 and 6.
- Proof: `accessor_query_projects_recipe_argument`, `accessor_value_is_never_stored`,
  `accessor_address_resolves_in_a_second_environment`, `structural_edit_expires_action_accessor_asset`,
  `document_field_projection`, `store_command_views`; `cargo test -p liquers-lib --test registry_export`.
- Rollback: unregister the namespace and regenerate the registry.

### Step 8: Record steps
- Files / symbols: `liquers-lib/src/records/accessor.rs` `RowStep`, `ColumnStep`, `CellStep`
  (`#[cfg(feature = "records")]`), with its module doc.
- Depends on: Step 6. Can run in parallel with Step 7.
- Proof: `record_row_column_and_cell_projections`; `cargo test -p liquers-lib --no-default-features --features records --lib --tests`.
- Rollback: remove the file.

### Step 9: The `/api/acc` routes and socket
- Files / symbols: new `liquers-axum/src/accessor/` (`AccessorApiBuilder<E>`, handlers under the
  `q/`, `info/`, `log/`, `describe/` and `ws/` prefixes, socket reusing `assets/websocket.rs`,
  module doc); the handlers answer `412`, `428` and `405` themselves; `server/config.rs`
  `Api::Accessors`, `Api::defaults()` (the default of `apis`); `server/builder.rs` `with_api`, the
  `{prefix}/api/acc` mounting with `read_only` and `without_websocket`, and the type-registry check
  in `bind`; `api_core` docs and the server module doc.
- Depends on: Step 7.
- Proof: `cargo test -p liquers-axum --test accessor_api` (needs `libssl-dev`, `CLAUDE.md`),
  including `accessors_api_is_opt_in_and_configurable`; `cargo test -p liquers-axum --lib server`
  and `--test server_config_document` (the default `apis` unchanged).
- Rollback: unmount the builder.

### Step 10: The Python wrapper
- Files / symbols: new `liquers-py/src/accessor.rs` (module doc; per-wrapper token; the conflict
  `token` parsed from the message), `lib.rs`, `value.rs`, `environment.rs`, `builtins.rs` (Step 7's
  macro); `liquers-py/python/liquers/accessors.py` and `.pyi`; `liquers-py/README.md` conformance
  rows; `tests/test_ACCESSOR_use.py`; `tests/test_LAYOUT_submodules.py` (`accessors`).
- Depends on: Step 7. Can run in parallel with Steps 9 and 11.
- Proof: `bash scripts/test-liquers-py.sh` (`test_ACCESSOR01`–`07`, `test_STUBS_declarations.py`).
- Rollback: remove the module and its registration.

### Step 11: The JavaScript wrapper
- Files / symbols: new `liquers-web/src/accessor.rs` (module doc; spawns `driver()` with
  `spawn_local` on the first subscription), `default_value.rs` arms, `environment.rs`
  (`accessor`, `memoryAccessor`), `builtins.rs` (Step 7's macro), `typescript.rs`;
  `tests/accessor_ACCESSOR.rs`.
- Depends on: Step 7. Can run in parallel with Steps 9 and 10.
- Proof: after `cargo clean`: `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test accessor_ACCESSOR`
  and `./liquers-web/scripts/check-stubs.sh --build`.
- Rollback: remove the class; the `default_value.rs` arms return `None` again.

### Step 12: Whole-change checks
- Files / symbols: none: checks only.
- Depends on: Steps 1–11.
- Rollback: none needed.
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
`scripts/check_api_docs.py` at each step that writes them and again at Step 12. The two new
documents join `affects_docs` in Phase 5. Regenerated: `specs/command_registry.yaml` (Step 7),
`specs/index.csv` and `specs/index.md` (`python3 scripts/docs_index.py`). Updated by hand:
`specs/README.md` (the reference and the guide).

## Execution Notes

Deviations from Phases 2 and 3 found while implementing, each kept small:

- **The writer's node also gets `Changed`.** Phase 1 Q4 said an unchanged part includes the one
  being edited. Handles are shared (an `Arc` clone is the same node), so another holder of the
  writer's own handle must hear of the set; every node whose content changed gets `Changed`.
- **Every live node is derived on a change**, not only the subscribed ones. Only a cost: a node
  with no subscriber still has its last-seen fingerprint, which a later `set` needs.
- **`AccessorRoot::write` returns the state as written** with its version (`(State<V>, Version)`),
  not only the version, so the hub caches the status the root chose (`Source`, `Override`) rather
  than the reset `None` it passed in. The hub resets the status before the write; roots do not.
- **The hub's watcher ends with the hub**: a `watch` channel dropped with the hub is raced against
  the root's wait, so a watcher blocked on a quiet asset does not outlive its accessors.
- **An asset-backed root relays the latest message on (re)subscription** (`mark_changed`), because
  the asset may have moved on between `AssetManager::get` and the subscription. Its info takes the
  secondary progress and the log length from the live asset's metadata, falling back to the last
  relayed secondary progress.
- **A new key written through the binary view** is described as the bytes value's own type and
  default format (`Bytes`, `b`), not the key's extension: opaque bytes cannot be checked against
  the extension's format. The value view uses the extension as Phase 2 says.
- **`StoreView::parse`** (`binary` / `metadata` / `value`) was added for the `acc/store` argument.
- **`ProjectionStep::overridden_arguments`** (defaulted, empty) lets `RecipeStep` name the
  arguments its recipe's `arguments` and `links` override; the hub's `describe` checks every
  ancestor, because an argument step sees only its action, never the recipe.
- **`PipelineEditor::argument_source`** (and the `QueryEditor` delegate) was added: the editor
  could set and unset an argument but not read one. A variadic argument reads as the plain JSON
  array of its items; a link or placeholder in its tagged JSON form; an argument the action does
  not give reads as `none`, and setting `none` unsets it.
- **The argument step works on the first action** of its parent (the one-element pipeline of an
  action step, or any query), and an action part keeps the parent's realm.
- **`hub_error_cases`** stays on the test steps of `hub.rs`; the real steps' error paths are in
  `refused_argument_set_writes_nothing`.
- **`SimpleValue` gained core `Value`'s JSON forms** for `Metadata`, `Recipe` and `Pipeline`. It
  refused them, so in `liquers-lib` a recipe part could not be read as a document and a
  metadata-view set could not read its record; core `Value` already had these forms.
- **`accessor_value_is_never_stored`** checks the store and the registry rather than
  `persistence_status`: `AssetManager::set_state` of a type with no formats writes the metadata
  only and leaves the asset's persistence status `None`. A query asset is not persisted at all.
- **The `Accessor` type info** has the defaults `accessor` / `value.accessor` and no formats, so the
  registry export lists no format for it.
- **Tests compare fingerprints, not `Changed`**, after an evaluation: the new asset's later status
  events replace `Changed` in the channel, which is the holder contract (§Events) at work.
- **A step's own error is reported** when a part cannot be derived (`missing_part_error`): the hub
  used to answer every failed derivation with "the part does not exist", which hid a step that
  failed to read its parent. A part that is simply absent (`Ok(None)`) still gets that message.
- **Record rows and columns are built as JSON** (`try_from_json_value`): `liquers-lib`'s `Value`
  has no `from_object` / `from_array`, so a row is a JSON object and a column a JSON array.
- **`PUT` bodies and `If-Match: *`.** A body is read in `?format=`, else the format its
  `Content-Type` names (`application/json`, `text/plain`, YAML, `application/octet-stream`), else
  JSON; `?type=` names its type identifier. `If-Match: *` (the value as it is now) was added: it
  reads the part and sets against that token, and answers `412` when the value does not exist.
- **`info` has no `etag` until the accessor has been read**, since it never reads the root: a
  client that needs a token reads `q/` first, or uses `If-Match: *`.
- **Resolving a route's query evaluates it**, for every route: only the value tells whether it is
  an accessor. An accessor query is cheap (no root value is read); a plain query's `info/`
  evaluates that query.
- **The socket closes with 1000 after `invalidated`**: the accessor is dead, and its query builds a
  new one. For a source that cannot notify (the store root) it refreshes once a second.
- **`liquers-axum` gained `liquers-lib` (no default features) and `liquers-macro` as
  dev-dependencies**, for a value type with accessors in `tests/accessor_api.rs`. The
  `removed` frame is not covered there: a removal needs a keyed asset deleted under a live socket,
  which the core tests cover (`expiry_and_removal_are_reported`).

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
