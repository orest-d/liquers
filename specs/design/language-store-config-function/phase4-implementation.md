# Phase 4: Implementation Plan — Language-defined store_config handlers

## Overview

Core first (steps 1-2), which alone delivers AC-8 and AC-9 and unblocks both bindings. Python
(steps 3-4) and JavaScript (steps 5-6) then proceed independently and may run in parallel or in
separate PRs. Written under Phase 1's recommended answers; a different answer to Q1 changes the
surface in steps 4 and 6, to Q2 the lifecycle in steps 3 and 5. No prerequisite issue.

## Progress

- [ ] Step 1: Closure builders in `realm.rs`
- [ ] Step 2: `literal_parameters` and core tests
- [ ] Step 3: Python handler type and environment lifecycle
- [ ] Step 4: Python decorator and tests
- [ ] Step 5: JavaScript handler type, retention and rebuild
- [ ] Step 6: JavaScript entry point, declarations and tests
- [ ] Step 7: Documentation and records

## Implementation Steps

### Step 1: Closure builders in `realm.rs`
- Files / symbols: `liquers-core/src/store/realm.rs` `StepBuilder`, `StoreConfigFunctions`
  (doc comment, `steps` map), `StoreConfigFunctions::register_step`, `StoreConfigFunctions::compile_plan`
  (the `(Some(builder), _) => *builder` arm and the `builder(stage, parameters)` call).
- Change: the `cfg` pair of `Arc<dyn Fn …>` aliases (Phase 2); `register_step` generic over `F`,
  storing `Arc::new(builder)`; `compile_plan` binds `builder` by reference. `ComponentBuilder`
  untouched.
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib store::` (AC-8, test 5), `cargo check -p liquers-core
  --target wasm32-unknown-unknown`, `cargo test -p liquers-lib --lib --tests`.
- Rollback: revert the file; no caller changed.

### Step 2: `literal_parameters` and core tests
- Files / symbols: `liquers-core/src/store/realm.rs` new `pub fn literal_parameters`, `tests`.
- Change: the helper (literal values in order; `MultipleParameters` as a JSON array; any link or
  placeholder is `Error::configuration_error`, matching `string_parameter`'s exhaustive match);
  tests 3 and 4.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-core --lib store::realm::tests` (AC-4 core half, AC-9).
- Rollback: remove the helper and tests.

### Step 3: Python handler type and environment lifecycle
- Files / symbols: new `liquers-py/src/store_config.rs` (`PyStoreConfigHandler`), `liquers-py/src/lib.rs`
  (module declaration), `liquers-py/src/environment.rs` (`Declarations::handlers`,
  `EnvCell::configured_builder`, new `EnvCell::register_handler`).
- Change: `PyStoreConfigHandler::new` composes metadata with `CommandDeclaration` as
  `PyCommandSpec::new` does and refuses an `async def` (`environment.rs` `is_coroutine_function`,
  made `pub(crate)`). Its builder converts
  `literal_parameters` to Python with `json.loads` once, under the GIL. Its step takes the GIL
  through `liquers-py/src/value.rs` `attach_off_worker`, made `pub(crate)` (it already does the
  `block_in_place`-on-a-multi-thread-runtime check). It wraps the record in `crate::metadata::MetadataRecord`,
  calls the function, reads back a `MetadataRecord` (or keeps the input on `None`), and converts an
  exception with `pyerr_to_error`. `register_handler` refuses with `Error::configuration_error` when
  `declarations.store` is `Some`, else replaces by key and calls `retire`.
- Depends on: Steps 1-2.
- Proof: `cargo check -p liquers-py`; the Rust unit tests run by `bash scripts/test-liquers-py.sh`.
- Rollback: revert the three files.

### Step 4: Python decorator and tests
- Files / symbols: `liquers-py/src/environment.rs` (`#[pymethods]` `store_config_handler`,
  `_register_store_config_handler`), `liquers-py/python/liquers/_commands.py` (handler decorator
  reusing `_baseline`), new `liquers-py/tests/test_STORE_config_handlers.py`.
- Change: the decorator with options `name`, `label`, `doc`; tests 6-11.
- Depends on: Step 3.
- Proof: `bash scripts/test-liquers-py.sh` — tests 6-11 and every existing test.
- Rollback: revert; step 3 is inert without a caller.

### Step 5: JavaScript handler type, retention and rebuild
- Files / symbols: new `liquers-web/src/store/config_handler.rs` (`JsStoreConfigHandler`, its
  builder), `liquers-web/src/store/mod.rs` (module), `liquers-web/src/environment.rs`
  (`REGISTERED_HANDLERS`, replay in `rebuild_with` / `rebuild_without` and the pending path before
  `apply_store`, new `register_store_config_handler_on`).
- Change: the step renders the record with `metadata_to_js_value(key, &Metadata::MetadataRecord(record.clone()), ..)`
  (it takes `&Metadata`), calls `run` with it and the arguments (`JSON.parse` of each literal), and
  reads the result: `undefined` or `null` keeps the input (checked *before* conversion, because
  `metadata_from_js_value` turns `undefined` into an empty record); a `js_sys::Promise` is an error;
  anything else goes through `metadata_from_js_value` (`JSON.stringify`, then
  `serde_json::from_str::<MetadataRecord>`) and must yield `Metadata::MetadataRecord(r)`, whose `r`
  replaces the record; `Metadata::LegacyMetadata` is an error. A throw is converted with
  `js_error_to_liquers(.., ErrorType::ExecutionError)`.
- Depends on: Steps 1-2.
- Proof: `cargo check -p liquers-web --target wasm32-unknown-unknown`.
- Rollback: revert; nothing calls it yet.

### Step 6: JavaScript entry point, declarations and tests
- Files / symbols: `liquers-web/src/lib.rs` (`registerStoreConfigHandler`), `liquers-web/src/typescript.rs`
  (declaration type), `liquers-web/src/command/spec.rs` (refuse realm `store_config` in
  `registerCommand`), `liquers-web/tests/store_js_STORE.rs` (tests 12-14).
- Change: as Phase 2.
- Depends on: Step 5.
- Proof: after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features
  debug-handles` (and with `,records`); `./liquers-web/scripts/check-stubs.sh --build`.
- Rollback: revert the four files.

### Step 7: Documentation and records
- Files: Phase 2 §Documentation Architecture; `liquers-web/README.md` (`STORE18` from `NA` to
  required and passing), `liquers-py/README.md`; the source feature file.
- Change: each `reference/` and `guides/` document gets a `## History` row and a `reviewed:` bump;
  the `STORE18` wording (Q2) and its Python blueprint (accessors that exist); close
  `LANGUAGE-STORE-CONFIG-FUNCTION-NOT-DEFINABLE` with a resolution note (§4.3); `specs/README.md`
  row; `python3 scripts/docs_index.py`, then `--check`.
- Depends on: Steps 1-6 (if one binding waits, its half becomes an issue, §5.6).
- Proof: `python3 scripts/docs_index.py --check`.
- Rollback: revert the documents.

## Testing Plan

After steps 1-2: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-lib --lib --tests`,
and the wasm `cargo check` of core. After steps 3-4: the Python script. After steps 5-6: the
`liquers-web` Node loops and the stub check, run separately after `cargo clean`. No `cfg(feature)`
or optional dependency changes, so `scripts/check-build-matrix.sh` is optional; run it once at the
end since `realm.rs` gains a `cfg(target_arch)` pair. `specs/command_registry.yaml` is unchanged,
and `cargo test -p liquers-lib --test registry_export` confirms it. Final diff review: no
`println!`, no `unwrap`/`expect` outside tests, no `_ =>` on Liquers enums.

## Rollback Plan

Each binding half reverts without touching core. Reverting core requires reverting both bindings
first. If only steps 1-2 land, Rust gains closure builders and nothing else changes.

## Documentation Updates

`specs/reference/STORE_CONFIG_FSD.md`, `specs/guides/STORE_CONFIG_GUIDE.md`,
`specs/reference/PYTHON_INTEGRATION.md`, `specs/guides/PYTHON_GUIDE.md`,
`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (History rows, `reviewed:` bumps); both binding
READMEs; `specs/README.md`; `specs/index.csv` regenerated.

## Phase 5 Entry Criteria

None — four-phase design.
