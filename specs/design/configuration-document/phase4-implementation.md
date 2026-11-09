# Phase 4: Implementation Plan — configuration-document

## Overview

Two parts, in the landing order of Phase 1: the core `ConfigDocument` first, with
`axum-server-assembly` (which adds `ServerConfig::from_document` in its Step 1); then liquers-py's
`PyConfig`, with `liquers-py-web-server`. Everything lands on branch
`claude/project-thread-5j5pvg` (PR #264) as two commit groups, so group A (core and liquers-axum) can
be split into its own PR if wanted. No issue must be fixed first.

## Progress

- [ ] Step 1: core `ConfigDocument` and its unit tests
- [ ] Step 2: liquers-py `PyConfig`, `config=` forms and the keyword rule
- [ ] Step 3: the command registry file in liquers-py
- [ ] Step 4: Python tests `test_CONFIG_document.py`
- [ ] Step 5: documentation and module docs

## Implementation Steps

### Step 1: core `ConfigDocument`

- Files / symbols: `liquers-core/src/environment_config.rs` — `ConfigDocument`,
  `ENVIRONMENT_KEYS`, `from_yaml`, `from_json`, `from_toml` (feature `toml`), `load`
  (`cfg(not(target_arch = "wasm32"))`), `take_section`, `finish`, `command_registry_path`.
- Change: as Phase 2 Interfaces. `from_yaml` parses to `serde_yaml::Value`, converts to
  `serde_json::Value` (refusing tags and non-string keys with `parse_error`), maps `null` to `{}`,
  moves `ENVIRONMENT_KEYS` into a map passed to `EnvironmentConfig::from_json`, takes
  `command_registry` (a string), keeps the rest in `sections`. `finish` names the untaken sections
  and the known ones (`ENVIRONMENT_KEYS`, `command_registry`, and every name `take_section` was
  called with, kept in a `taken` list).
- Depends on: none.
- Proof: `cargo test -p liquers-core --lib environment_config` — `document_splits_environment_and_sections`,
  `finish_refuses_an_untaken_section`, `environment_keys_match_the_struct`,
  `empty_document_is_the_default`, `document_refuses_tags_in_sections`,
  `document_refuses_non_string_keys`, `load_refuses_an_unknown_extension`; and
  `cargo check -p liquers-core --target wasm32-unknown-unknown` (no `load` there).
- Rollback: revert the commit; nothing else uses the type yet.

### Step 2: liquers-py `PyConfig`, `config=` forms and the keyword rule

- Files / symbols: new `liquers-py/src/config.rs` (`PyConfig`, `PythonSettings`);
  `liquers-py/src/environment.rs` — `parse_config` returns `PyConfig`; `Environment::new` keywords
  `conversion`, `pickle`, `async_loops`, `job_capacity` become `Option`; `EnvCell::new(config:
  PyConfig, …)` and its call sites (`environment.rs:731`, `:1071`, `records/resolver.rs:186`).
- Change: `os.PathLike` (has `__fspath__`) → `ConfigDocument::load`; `str` → `from_yaml`, with the
  AC-5 hint when the result is a scalar; `dict` → `json.dumps` → `from_json`. Resolution keyword →
  `python:` → default; `job_capacity` per Decision 2 (keyword wins, if confirmed; otherwise the
  existing conflict check stays). Needs `ServerConfig::from_document` (`axum-server-assembly` Step 1)
  and `liquers-axum` as a dependency (`liquers-py-web-server` Step 1).
- Depends on: Step 1; `axum-server-assembly` Step 1; `liquers-py-web-server` Step 1.
- Proof: `cargo check -p liquers-py`; `bash scripts/test-liquers-py.sh` with the existing suite green
  except `test_ASYNC_loops.py`'s conflict case, moved in Step 4.
- Rollback: revert; `EnvCell` goes back to `EnvironmentConfig`.

### Step 3: the command registry file in liquers-py

- Files / symbols: `environment.rs` — the build reads `PyConfig::command_registry` and calls
  `EnvCell::import_metadata` with its `commands`.
- Change: read and parse at construction (errors raise there, naming the path); apply on every
  build, as imported metadata is today.
- Depends on: Step 2.
- Proof: `test_CONFIG_command_registry_file`, `test_CONFIG_missing_command_registry_file`.
- Rollback: revert the step; the field is then parsed and ignored.

### Step 4: Python tests

- Files / symbols: new `liquers-py/tests/test_CONFIG_document.py` (Phase 3 Test Plan, Python);
  `liquers-py/tests/test_ASYNC_loops.py` — the conflict case moves to
  `test_CONFIG_keywords_override_the_file` (Decision 2).
- Depends on: Steps 2–3; `liquers-py-web-server` Step 4 for the tests that call `serve`.
- Proof: `bash scripts/test-liquers-py.sh`.
- Rollback: revert the test files with the steps they cover.

### Step 5: documentation and module docs

- Files: `specs/reference/ENVIRONMENT_CONFIG.md` (§The configuration document, History row,
  `reviewed:`), `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` ("Configure from one file"),
  `specs/reference/PYTHON_INTEGRATION.md` and `specs/guides/PYTHON_GUIDE.md` (`config=` forms,
  `python:`, the override rule), the module docs of `environment_config.rs` and `config.rs`.
- Depends on: Steps 1–4.
- Proof: `python3 scripts/check_api_docs.py liquers-core/src/environment_config.rs
  liquers-py/src/config.rs`; `python3 scripts/docs_index.py --check`.
- Rollback: revert.

## Testing Plan

Step 1's unit tests run after Step 1; the Python suite after Steps 2–4. Final: `cargo test -p
liquers-core --lib --tests`, `bash scripts/test-liquers-py.sh`, and the wasm32 row of
`scripts/check-build-matrix.sh` for the `cfg` on `load`.

## Rollback Plan

Each part reverts on its own: without group B, the core type is unused but harmless; without group
A, group B cannot build, which is why they land in this order.

## Documentation Updates

As Step 5; `specs/index.csv` regenerated with `python3 scripts/docs_index.py`; no command registry
change.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
