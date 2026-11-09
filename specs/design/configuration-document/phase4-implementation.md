# Phase 4: Implementation Plan — configuration-document

## Overview

Two parts, in the landing order of Phase 1: the core `ConfigDocument` first, with
`axum-server-assembly` (which adds `ServerConfig::from_document` in its Step 1 and carries this
design's AC-6/AC-7 Rust test in its Step 6); then liquers-py's `PyConfig`, with
`liquers-py-web-server`. Everything lands on branch `claude/project-thread-5j5pvg` (PR #264) as two
commit groups, so group A (core and liquers-axum) can be split into its own PR if wanted. No issue
must be fixed first. Decision 2 was answered "keyword wins" (2026-11-09).

## Progress

- [ ] Step 1: core `ConfigDocument` and its unit tests
- [ ] Step 2: liquers-py `PyConfig`, `config=` forms, the keyword rule and the stubs
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
  `finish_refuses_an_untaken_section`, `environment_keys_match_the_struct` (compares
  `serde_json::to_value` of both configurations, since `EnvironmentConfig` has no `PartialEq`),
  `empty_document_is_the_default`, `document_refuses_tags_in_sections`,
  `document_refuses_non_string_keys`, `load_refuses_an_unknown_extension`; the same with
  `--features toml` (a `.toml` document) and `--no-default-features`; and `cargo check -p
  liquers-core --target wasm32-unknown-unknown` (no `load` there; needs `rustup target add
  wasm32-unknown-unknown`).
- Rollback: revert the commit; nothing else uses the type yet.

### Step 2: liquers-py `PyConfig`, `config=` forms, the keyword rule and the stubs

- Files / symbols: new `liquers-py/src/config.rs` (`PyConfig`, `PythonSettings`);
  `liquers-py/src/environment.rs` — `parse_config` returns `PyConfig`; `Environment::new` keywords
  `conversion`, `pickle`, `async_loops`, `job_capacity` become `Option`; `EnvCell::new(config:
  PyConfig, …)` and its call sites (`environment.rs:731`, `:1071`, `records/resolver.rs:186`);
  `python/liquers/__init__.pyi:1034` — `config: os.PathLike[str] | dict[str, Any] | str | None`
  and the four keywords `… | None = ...`.
- Change: `os.PathLike` (has `__fspath__`) → `ConfigDocument::load`; `str` → `from_yaml`, with the
  AC-5 hint when the result is a scalar; `dict` → `json.dumps` → `from_json`. Resolution: keyword →
  `python:` section → default. `job_capacity` per Decision 2 (keyword wins): `job_capacity=`
  replaces `assets.job_capacity`, and the conflict case of `test_ASYNC_loops.py` moves to
  `test_CONFIG_keywords_override_the_file` (Step 4). Needs `ServerConfig::from_document` (`axum-server-assembly` Step 1) and `liquers-axum` as a
  dependency (`liquers-py-web-server` Step 1).
- Depends on: Step 1; `axum-server-assembly` Step 1; `liquers-py-web-server` Step 1.
- Proof: `cargo check -p liquers-py`; `bash scripts/test-liquers-py.sh` with the existing suite
  green (except `test_ASYNC_loops.py`'s conflict case, moved in Step 4),
  including `test_STUBS_declarations.py`, which runs only on CPython 3.14 with the GIL (run the
  script with `PYTHON=python3.14`).
- Rollback: revert; `EnvCell` goes back to `EnvironmentConfig`.

### Step 3: the command registry file in liquers-py

- Files / symbols: `environment.rs` — the build reads `PyConfig::command_registry` and calls
  `EnvCell::import_metadata` with its `commands`.
- Change: read and parse at construction (errors raise there, naming the path); apply on every
  build, as imported metadata is today.
- Depends on: Step 2.
- Proof: an interim check in the venv (Example 2's files: `env.command_metadata("greet").label`
  is `Greet someone`; a missing file raises naming it); the tests
  `test_CONFIG_command_registry_file` and `test_CONFIG_missing_command_registry_file` land in
  Step 4.
- Rollback: revert the step; the field is then parsed and ignored.

### Step 4: Python tests

- Files / symbols: new `liquers-py/tests/test_CONFIG_document.py` (Phase 3 Test Plan, Python),
  its file fixtures from `tmp_path` in the file itself; `liquers-py/tests/test_ASYNC_loops.py` — the conflict case moves to `test_CONFIG_keywords_override_the_file`.
- Depends on: Steps 2–3; `liquers-py-web-server` Step 4 for the tests that call `serve`.
- Proof: `bash scripts/test-liquers-py.sh`.
- Rollback: revert the test files with the steps they cover.

### Step 5: documentation and module docs

- Files: `specs/reference/ENVIRONMENT_CONFIG.md` (§The configuration document, History row,
  `reviewed:`), `specs/guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md` ("Configure from one file"),
  `specs/reference/PYTHON_INTEGRATION.md` and `specs/guides/PYTHON_GUIDE.md` (`config=` forms,
  `python:`, the override rule), `docs/guides/python/08-environment.md` and
  `docs/guides/rust/08-environment.md` (the configuration file), the module docs of
  `environment_config.rs` and `config.rs`; `phase5-documentation.md`.
- Depends on: Steps 1–4.
- Proof: `python3 scripts/check_api_docs.py liquers-core/src/environment_config.rs
  liquers-py/src/config.rs`; `cargo run -p liquers-docs -- check`;
  `python3 scripts/docs_index.py --check`.
- Rollback: revert.

## Testing Plan

Step 1's unit tests after Step 1, in the core configurations (default, `--no-default-features`,
`--features toml`, wasm32 check); AC-6/AC-7 in Rust with `axum-server-assembly` Step 6
(`cargo test -p liquers-axum --test server_config_document`); the Python suite after Steps 2–4.
Final: `cargo test -p liquers-core --lib --tests`, `cargo test -p liquers-axum --test
server_config_document`, `bash scripts/test-liquers-py.sh`, and the wasm32 row of
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
