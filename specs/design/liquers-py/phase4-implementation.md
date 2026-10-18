# Phase 4: Implementation Plan — liquers-py

## Overview

Steps 1 and 2 are the additive changes below `liquers-py`. They can run in parallel, and each
carries its reference-document edit. The other steps are sequential. Each one leaves the crate
compiling, and each is proven by the earliest Phase 3 tests its code makes runnable. There are no
prerequisite issues.

## Progress

- [x] Step 1: foreign decoder hook in `liquers-lib` — 9b2ba9fe
- [x] Step 2: formatless read rule in `liquers-core` — 1f17bde3
- [x] Step 3: `liquers-py` dependencies and runtime — 0d0a4c13
- [x] Step 4: value bridge, with the old environment removed — 0d0a4c13
- [x] Step 5: error mapping — e914dbf1
- [x] Step 6: environment — 3cc4e0b7
- [x] Step 7: commands and decorators — 2a987613
- [x] Step 8: context for nested evaluation — 2a987613
- [x] Step 9: `State`, `AssetRef`, `Status` — 620770b5
- [x] Step 10: `debug-handles` and the test script — 4fd469a8
- [ ] Step 11: remaining Appendix A suites and the conformance matrix
- [ ] Step 12: final checks

## Implementation Steps

### Step 1: foreign decoder hook in `liquers-lib`
- **Files / symbols:**
  - `liquers-lib/src/value/foreign.rs`: `ForeignDecoder`, `register_foreign_decoder` and
    `foreign_decoder`, kept in a `OnceLock<RwLock<HashMap>>`;
  - `liquers-lib/src/value/mod.rs`: the `_` arm of `ExtValue::deserialize_from_bytes`;
  - `specs/reference/VALUE_TYPE_SYSTEM.md`: the hook, with a History row and a `reviewed:` bump.
- **Depends on:** none.
- **Proof:** `cargo test -p liquers-lib --lib value::foreign`
  (`decoder_rebuilds_registered_identifier`).
- **Rollback:** revert. Nothing calls the hook until step 4.

### Step 2: formatless read rule in `liquers-core`
- **Files / symbols:**
  - `liquers-core/src/assets.rs` `deserialize_stored_value`, plus the degraded messages at
    `:1430` and `:7487`. The test goes in the top-level `mod tests` (`:10527`).
  - `specs/reference/ASSETS.md`: the rule, with a History row and a `reviewed:` bump.
- **Changed while implementing:** the read rule is documented where the read path already is,
  `VALUE_TYPE_SYSTEM.md` §Reading, rather than in `ASSETS.md`, which does not describe it. Run
  without waiting for the owner's answer on the read-rule card, on the recommended option, as the
  owner instructed on 2026-10-18 ("follow all the phases, no need to wait for approval").
  A degraded key with no recipe is still reported as "No recipe found", filed as
  `DEGRADED-STORED-VALUE-WITHOUT-RECIPE-REPORTS-NO-RECIPE`.
- **Depends on:** none; can run in parallel with step 1.
- **Proof:**
  - `cargo test -p liquers-core --lib assets::tests::formatless`;
  - `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests` stay green.
- **Rollback:** revert. AC-18 reopens.

### Step 3: `liquers-py` dependencies and runtime
- **Files / symbols:**
  - `liquers-py/Cargo.toml`:
    - `liquers-lib` with `default-features = false` and the features `image-support`, `polars`,
      `records`, `records-ipc`, `records-parquet` and `templates`;
    - `liquers-macro`, `tokio` (`rt-multi-thread`), `futures`, `async-trait` and `serde_json`,
      because the domain macros expand in this crate (`liquers-web/src/builtins.rs` is the precedent).
  - `src/runtime.rs` `runtime()`.
  - The comment on the liquers-py row in `scripts/check-build-matrix.sh`.
- **Depends on:** none.
- **Proof:**
  - `cargo check -p liquers-py --lib --tests`;
  - `bash scripts/test-liquers-py.sh` still passes the M0 suites.
- **Rollback:** revert.

### Step 4: value bridge, with the old environment removed
- **Files / symbols:**
  - `src/value.rs` (rewritten): `PyObjectValue`, `py_type_info`, `ConversionMode`, `value_to_py`
    and `py_to_value`, with the decoder registered in `_native`'s init.
  - `src/context.rs`: the old `Environment` and `Context`, which implement core traits over the
    removed enum, are deleted here and replaced in step 8.
- **Depends on:** steps 1 and 3.
- **Proof:** `cargo test -p liquers-py --no-default-features --lib value`
  (`py_type_info_agrees_with_instance` and the conversion-table tests).
- **Rollback:** revert steps 4 onwards together.

### Step 5: error mapping
- **Files / symbols:**
  - `python/liquers/_errors.py`: `LiquersError` and its generated `<Variant>Error` subclasses;
  - `src/error.rs`: `error_to_pyerr` and `pyerr_to_error`, covering the class, the message, the
    traceback and `BaseException`.
- **Depends on:** step 4.
- **Proof:** `cargo test -p liquers-py --no-default-features --lib error`, round-tripping every
  `ErrorType` and every field.
- **Rollback:** revert.

### Step 6: environment
- **Files / symbols:**
  - `src/environment.rs`: `Environment`, `EnvCell`, `EnvState`, `build`, `evaluate`,
    `evaluate_state`, `store_set`, `validate`, `import_metadata` and `shutdown`;
  - `src/builtins.rs`: the six domain macros, with `type CommandEnvironment = DefaultEnvironment<Value>`
    and the `Context` import;
  - `python/liquers/_env.py` `default_environment`.
  - Added while implementing: an `Issue` pyclass (`severity`, `message`, `key`) for `validate()`,
    and `liquers-store` as a direct dependency (already in the graph through `liquers-lib`) for
    its store factory chain, so a configured store can name the OpenDAL types too.
- **Build sequence:**
  1. `liquers_lib::environment::default_environment_builder::<Value, ()>()`. Kind `LibKind`
     matches the macros' alias and keeps the default recipe chain.
  2. `with_type_registry` with `py_type_info(pickle)`.
  3. On the first build, `with_store_config(config.store, default_store_factory())` when a store is
     configured, otherwise `with_async_store` of a memory store. Every rebuild instead passes
     `with_async_store(old_envref.get_async_store())`, so the data survives.
  4. `with_asset_manager_options(config.assets)`. A `config.recipes` other than the default is
     refused with `ConfigurationError`, because recipes belong to the STORE/RECIPE design.
  5. `builtins::register(&mut builder.command_registry)`.
  6. Imported metadata: parsed into a struct whose fields all default, then `add_command` for each
     entry. An entry with no executor is a validation issue.
  7. The retained Python specs (step 7).
  8. `build()` under `runtime().enter()`.

  A rebuild moves the old `EnvRef` to the retired list.
- **Depends on:** step 5.
- **Proof:** `test_ENVIRON01_default_environment_evaluates_builtin` (`commands_doc`, AC-2) and
  `test_EVAL04_invalid_query_maps_through_error` (`liquers-py/tests/`), run with
  `bash scripts/test-liquers-py.sh`.
- **Rollback:** revert.

### Step 7: commands and decorators
- **Files / symbols:**
  - `src/command.rs`: `PyCommandSpec`, `register_py_command` and `Replaced`. `impl_version` is set
    from a hash of `__code__.co_code` and `co_consts`.
  - `python/liquers/_commands.py`: `command` and `first_command`, plus the `Environment.command`,
    `first_command` and `command_metadata` methods.
  - Getters on `src/command_metadata.rs`: `label`, `doc` and `arguments` on `CommandMetadata`;
    `name` and `default` on `ArgumentInfo`.
  - `tests/conftest.py`: the fixture commands.
- **Added while implementing (steps 7 and 8 landed as one commit):**
  - `src/asset.rs` `State` is pulled forward from step 9, because `StateDelivery::State` needs it;
  - the build order is built-ins, then unregistrations, then Python specs, then imported metadata,
    so imported metadata can describe a Python command (step 6 had it before the specs);
  - the implementation hash also covers `co_names`, so a call to a different function counts;
  - the decorators also take `filename=`;
  - `evaluate` extracts with `state.value()`, because `AssetRef::get` returns a failed asset as
    an `Ok` error state. liquers-web has the same shape, filed as
    `WEB-EVALUATE-RESOLVES-FAILED-ASSET-AS-NULL`.
- **Depends on:** step 6.
- **Proof:**
  - COMMAND01–06 and 08–14 (AC-1, AC-9, AC-10), plus `test_COMMAND_late_registration_keeps_store`;
  - ERROR01–06 (AC-7, AC-8);
  - ENVIRON02–08 (AC-11, AC-12, AC-20) and EVAL01, 02 and 04;
  - VALUE01, 03–06 and 08 (AC-3–AC-6), plus `test_VALUE_pickle_off_never_unpickles` (AC-18);
  - `test_EVAL_text_resource_read_from_store` (AC-17).
- **Rollback:** revert. `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` reopens.

### Step 8: context for nested evaluation
- **Files / symbols:** `src/context.rs` `Context`: `evaluate`, `info`, `warning`, `error` and
  `is_cancelled`.
- **Depends on:** step 7.
- **Proof:** `test_RUNTIME04` (AC-13), `test_COMMAND07`, `test_EVAL05` and
  `test_RUNTIME_two_threads_evaluate_concurrently`.
- **Rollback:** revert.

### Step 9: `State`, `AssetRef`, `Status`
- **Files / symbols:**
  - `src/asset.rs`: `State`, `AssetRef` and `Environment.get_asset`;
  - `src/metadata.rs` `Status::new`: every core variant listed, a `ValueError` for an unknown
    name, and `__eq__`.
- **Added while implementing:** `Metadata.log()`, since the wrapper had no way to read the log,
  and `evaluate`, `evaluate_state`, `get_asset` and `Context.evaluate` accept an `lq.Query` as
  well as a string (EVAL02).
- **Depends on:** step 8, because the `slow` fixture uses `is_cancelled`.
- **Proof:** `test_EVAL03` and `test_EVAL06` (AC-19), plus `test_VALUE13`.
- **Rollback:** revert.

### Step 10: `debug-handles` and the test script
- **Files / symbols:**
  - the `debug-handles` feature and `Environment.live_handle_count`;
  - `scripts/test-liquers-py.sh`: install `polars` and `pillow`, then run
    `maturin develop --features debug-handles`.
- **Found while implementing:** a *built* environment never releases its callables, because the
  environment and its asset manager hold each other with strong `Arc`s
  (`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`, design in implementation). `RUNTIME05` therefore asserts
  what is released today (an environment that was never built), and its built-environment half is
  a strict `xfail` marked BLOCKED with that issue, so it fails loudly once the cycle is fixed.
  Retired environments leak the same way, so each late registration costs one environment.
- **Depends on:** step 9.
- **Proof:** `test_RUNTIME05`, and `test_VALUE08` running rather than skipped.
- **Rollback:** revert. RUNTIME05 is marked BLOCKED.

### Step 11: remaining Appendix A suites and the conformance matrix
- **Files / symbols:**
  - the remaining `liquers-py/tests/test_*` suites: OBJECT05–09, VALUE02, 07 and 09–11, and
    RUNTIME01, 03 and 06;
  - `liquers-py/README.md`: the matrix, with `RUNTIME02` and `VALUE12` as NA.
- **Change:** a local OBJECT audit failure is fixed in this step. Anything larger is triaged, filed,
  and its test marked BLOCKED with the issue ID.
- **Depends on:** step 10.
- **Proof:** `bash scripts/test-liquers-py.sh` passes in full (AC-21).
- **Rollback:** none needed, since the step adds only tests and docs.

### Step 12: final checks
- **Proof:**
  - `bash scripts/test-liquers-py.sh`;
  - `cargo test -p liquers-core --lib --tests` and `cargo test -p liquers-lib --lib --tests`;
  - `cargo test -p liquers-lib --test registry_export`, which changes nothing because no command
    signature changes;
  - `bash scripts/check-build-matrix.sh`, run with its low-disk cleans because the liquers-py row
    now builds polars;
  - `python3 scripts/docs_index.py --check`.
- **Depends on:** step 11.
- **Rollback:** n/a.

## Testing Plan

Each step's proof runs when the step lands. The Python suites run only through
`scripts/test-liquers-py.sh`. Rust tests run per crate: `--no-default-features` for liquers-py's own
unit tests, because the `extension-module` feature does not link libpython. Never use `--workspace`.

Expect about 8–10 GB of build output with the routine liquers-lib loop. Clean
`target/debug/incremental` before the matrix run.

## Rollback Plan

Every step is one commit, and every prefix of the steps compiles. Steps 1 and 2 stand alone. If the
work stops part-way, the unticked remainder becomes an issue (§5.6) and the matrix marks those
features `IN-PROGRESS`.

## Documentation Updates

- Steps 1 and 2 carry the two reference edits.
- Phase 5 rewrites `LANGUAGE-INTEGRATION_GUIDE.md` §6 and corrects §5 VALUE, both with a History
  row and a `reviewed:` bump.
- Phase 5 also updates `CLAUDE.md` (liquers-py notes) and `specs/README.md`.
- Step 11 writes `liquers-py/README.md`.
- `specs/index.csv` is regenerated by each step that touches `specs/`.
- `specs/command_registry.yaml` is unchanged.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
