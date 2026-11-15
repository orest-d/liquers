# Phase 4: Implementation Plan — py-command-line-config

## Overview

Core first (`overlay`), then the native keyword, then the Python layer, then the documents. It
depends on `py-default-environment-config` Step 4 (`configure`) and lands in the same PR.

## Progress

- [ ] Step 1: `liquers-core/src/environment_config.rs` `ConfigDocument`: the raw environment map,
  `overlay`, and `load`'s missing-file error, with the four core tests — `cargo test -p
  liquers-core --lib environment_config`
- [ ] Step 2: `liquers-py/src/config.rs` `PyConfig::from_python(.., overrides)`; `Settings::resolve`,
  `Environment.__new__` and `configure` gain `overrides=None` — `cargo test -p liquers-py --lib`
- [ ] Step 3: `liquers-py/python/liquers/_cli.py` (new), re-exported from `__init__.py`; stubs in
  `__init__.pyi` — `pytest tests/test_CLI_startup.py tests/test_STUBS_declarations.py`
- [ ] Step 4: Docs: chapter 8 "Starting an application"; YAML in every server and UI example of
  chapters 11 and 12; `PYTHON_GUIDE.md`, `PYTHON_INTEGRATION.md`, `ENVIRONMENT_CONFIG.md` —
  `pytest tests/test_GUIDE_examples.py`, `cargo run -p liquers-docs -- check`

## Implementation Steps

Each step lists its files, change and proof in the Progress line above, and depends on the one
before it. Step 1 can start in parallel with `py-default-environment-config`. To roll back a step,
revert its commit.

## Testing Plan

- `python3 scripts/check_api_docs.py` on `_cli.py`, `config.rs` and `environment_config.rs`.
- `cargo test -p liquers-core --lib`, since core changed. `bash scripts/check-build-matrix.sh` is
  not needed: no `cfg(feature)` or optional dependency changes. The wasm32 build of
  `liquers-core` is covered, because `overlay` is not gated and `load` is already
  `cfg(not(wasm32))`.
- `bash scripts/test-liquers-py.sh` once at the end.
- History rows and `reviewed:` bumps on the changed `reference/` and `guides/` documents, a row in
  `docs/guides/python/HISTORY.md`, and `python3 scripts/docs_index.py --check`.

## Rollback Plan

Revert the commits in reverse order. If Step 1 lands alone, `overlay` is unused, and the only
visible change is the missing-file error type.

## Documentation Updates

These are listed in Phase 2's Documentation Architecture, and the History rows and `reviewed:`
bumps are listed above.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
