# Phase 4: Implementation Plan — liquers-py

## Overview

The work goes from the bottom up. It starts with the two additive changes below `liquers-py`
(steps 1–2), which can be tested without Python. Next come the runtime and value bridge (3–4), the
error mapping (5), the environment (6), commands (7), context (8) and asset handles (9), each
proven by its Phase 3 suite. The work closes with the test-only handle counter, the remaining
Appendix A suites, and the conformance matrix (10–11), followed by final checks (12).

There are no prerequisite issues; Phase 2's preflight found no blocker. Steps 1 and 2 can run in
parallel. All the others are sequential.

## Progress

- [ ] Step 1: foreign decoder hook in `liquers-lib`
- [ ] Step 2: formatless read rule in `liquers-core`
- [ ] Step 3: `liquers-py` dependencies and runtime
- [ ] Step 4: value bridge and `PyObjectValue`
- [ ] Step 5: error mapping
- [ ] Step 6: environment
- [ ] Step 7: commands and decorators
- [ ] Step 8: context for nested evaluation
- [ ] Step 9: `State`, `AssetRef`, `Status`
- [ ] Step 10: `debug-handles` and the test script
- [ ] Step 11: remaining Appendix A suites and the conformance matrix
- [ ] Step 12: final checks

## Implementation Steps

### Step 1: foreign decoder hook in `liquers-lib`
- Files / symbols:
  - `liquers-lib/src/value/foreign.rs`: `ForeignDecoder`, `register_foreign_decoder`,
    `foreign_decoder`;
  - `liquers-lib/src/value/mod.rs`: the `_` arm of `ExtValue::deserialize_from_bytes`.
- Change: the registry is `OnceLock<RwLock<HashMap<&'static str, ForeignDecoder>>>`. The lookup
  comes before the unknown-identifier error, and the decoder's value is wrapped as
  `ExtValue::Foreign`.
- Depends on: none
- Proof: `cargo test -p liquers-lib --lib value::foreign`, through
  `decoder_rebuilds_registered_identifier`.
- Rollback: revert the commit. Nothing else calls it until step 4.

### Step 2: formatless read rule in `liquers-core`
- Files / symbols: `liquers-core/src/assets.rs` `deserialize_stored_value`, plus the two degraded
  messages near `assets.rs:1426` and `assets.rs:7486`.
- Change: a registered type with empty `supported_data_formats` returns `Ok(None)`, and the messages
  distinguish an unregistered type from a formatless one.
- Depends on: none (parallel with step 1)
- Proof:
  - `cargo test -p liquers-core --lib assets::tests::formatless`;
  - `cargo test -p liquers-core --lib --tests` stays green;
  - `cargo test -p liquers-lib --lib --tests` stays green.
- Rollback: revert the commit, which reopens AC-18.

### Step 3: `liquers-py` dependencies and runtime
- Files / symbols:
  - `liquers-py/Cargo.toml`: add `liquers-lib` (default features minus `egui`) and `tokio`
    (`rt-multi-thread`);
  - `liquers-py/src/runtime.rs`: `runtime()`.
- Depends on: none
- Proof: `cargo check -p liquers-py --lib --tests`; the M0 pytest suites still pass.
- Rollback: revert. The crate builds as at M0.

### Step 4: value bridge and `PyObjectValue`
- Files / symbols:
  - `liquers-py/src/value.rs` (rewritten): `PyObjectValue`, `py_type_info`, `ConversionMode`,
    `value_to_py`, `py_to_value`, with the decoder registered in `_native`'s init;
  - `liquers-py/src/recipes.rs`: ported off the own enum.
- Depends on: steps 1 and 3
- Proof:
  - `cargo test -p liquers-py --lib value`, through `py_type_info_agrees_with_instance` and
    conversion-table tests;
  - pytest `test_VALUE01`, `03`, `04` once step 6 exists.
- Rollback: revert steps 4 onward together. The bridge is their base.

### Step 5: error mapping
- Files / symbols:
  - `liquers-py/python/liquers/_errors.py`: `LiquersError` and its generated subclasses;
  - `liquers-py/src/error.rs`: `error_to_pyerr`, `pyerr_to_error` (class, message, `traceback`,
    `BaseException`).
- Depends on: step 4
- Proof: `test_ERROR_errors.py` ERROR01, 02, 04, 05; ERROR03 and 06 after step 7.
- Rollback: revert. Errors surface untyped.

### Step 6: environment
- Files / symbols:
  - `liquers-py/src/environment.rs`: `Environment`, `EnvCell`, `EnvState`, `build`, `evaluate`,
    `evaluate_state`, `store_set`, `validate`, `import_metadata`, `shutdown`;
  - `python/liquers/_env.py`: `default_environment`.
- Change:
  - `build` runs under `runtime().enter()` and registers the six liquers-lib domain macros.
  - Rebuilds retire the old `EnvRef`.
- Depends on: steps 4 and 5
- Proof:
  - `test_ENVIRON01`, 02–08 (ENVIRON02 and 07 completed after step 9);
  - `test_EVAL01`, 02, 04;
  - `test_EVAL_text_resource_read_from_store` (AC-17);
  - `test_VALUE06*` (AC-6, AC-18).
- Rollback: revert. The previous steps stand alone.

### Step 7: commands and decorators
- Files / symbols:
  - `liquers-py/src/command.rs`: `PyCommandSpec`, `register_py_command`, `Replaced`;
  - `python/liquers/_commands.py`: `command`, `first_command`;
  - `Environment.command`, `Environment.first_command`, `Environment.command_metadata`;
  - the `label`, `doc` and `arguments` getters on `command_metadata.rs` `CommandMetadata`.
- Depends on: step 6
- Proof: `test_COMMAND_registration.py` COMMAND01–14, AC-1, AC-9, AC-10;
  `test_COMMAND_late_registration_keeps_store`.
- Rollback: revert. `PY-ADD-PYTHON-COMMAND-ALIASES-AN-UNREGISTERED-COMMAND` reopens.

### Step 8: context for nested evaluation
- Files / symbols: `liquers-py/src/context.rs` (rewritten): `Context` with `evaluate`, `info`,
  `warning`, `error` and `is_cancelled`.
- Depends on: step 7
- Proof: `test_RUNTIME04` (AC-13), `test_COMMAND07`, `test_EVAL05`,
  `test_RUNTIME_two_threads_evaluate_concurrently`.
- Rollback: revert. Commands then run without a context.

### Step 9: `State`, `AssetRef`, `Status`
- Files / symbols:
  - `liquers-py/src/asset.rs`: `State`, `AssetRef`, `Environment.get_asset`;
  - `liquers-py/src/metadata.rs` `Status`: `__eq__` and `name`.
- Depends on: step 6
- Proof: `test_EVAL03`, `test_EVAL06` (AC-19), `test_VALUE13`.
- Rollback: revert.

### Step 10: `debug-handles` and the test script
- Files / symbols:
  - `liquers-py/Cargo.toml` feature `debug-handles`;
  - `Environment.live_handle_count`;
  - `scripts/test-liquers-py.sh`: install `polars` and `pillow`, run
    `maturin develop --features debug-handles`.
- Depends on: steps 7 and 9
- Proof: `test_RUNTIME05`, and `test_VALUE08` / AC-3 running rather than skipping.
- Rollback: revert. RUNTIME05 is marked BLOCKED.

### Step 11: remaining Appendix A suites and the conformance matrix
- Files / symbols:
  - `liquers-py/tests/test_*`: OBJECT05–09, VALUE02, 07, 09–11, ERROR, RUNTIME01, 03, 06, and the
    rest of COMMAND;
  - `liquers-py/README.md`: the conformance matrix, with `RUNTIME02` and `VALUE12` as NA and their
    reversing conditions.
- Change: an OBJECT audit failure in an existing wrapper is fixed here if it is local. Otherwise it
  is filed (triage), and the test is marked BLOCKED with the issue ID.
- Depends on: step 10
- Proof: `bash scripts/test-liquers-py.sh` fully green (AC-21).
- Rollback: none needed; this step only adds tests and docs.

### Step 12: final checks
- Proof:
  - `bash scripts/check-build-matrix.sh` (the liquers-py row and the liquers-lib rows touched by
    step 1);
  - `cargo test -p liquers-lib --lib --tests`;
  - `python3 scripts/docs_index.py --check`;
  - `cargo test -p liquers-lib --test registry_export`, which must stay green (no command
    signature changes).
- Depends on: step 11
- Rollback: n/a

## Testing Plan

Each step's proof runs when the step lands. Python suites run through
`bash scripts/test-liquers-py.sh`. Rust tests run per crate, never with `--workspace`.

`liquers-py` now links `liquers-lib`, so its build is about the size of the
`cargo test -p liquers-lib` loop. Run `cargo clean -p liquers-py` between feature changes if disk
runs short (CLAUDE.md, "No space left on device").

The build matrix runs once, in step 12, because step 1 touches a module that is compiled in every
configuration.

## Rollback Plan

Every step is a separate commit, ordered so that a prefix of the steps is always a working crate.
Steps 1 and 2 are independent of Python and stay valid if liquers-py work stops. If only some steps
land, the unticked remainder becomes an issue (§5.6), and the matrix marks the unfinished features
`IN-PROGRESS`.

## Documentation Updates

- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: §6 Python rewritten and the §5 VALUE paragraph
  corrected.
- `specs/reference/VALUE_TYPE_SYSTEM.md`: the foreign decoder hook.
- `specs/reference/ASSETS.md`: the formatless read rule.
- Each of the three documents above gets a `## History` row and a `reviewed:` bump in the same
  commit as its change.
- `liquers-py/README.md`: quick start and matrix.
- `specs/README.md`: the design status.
- `specs/index.csv`: regenerated with `python3 scripts/docs_index.py`.
- `specs/command_registry.yaml`: unchanged, since no `register_command!` signature changes.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
