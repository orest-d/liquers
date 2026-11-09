# Phase 4: Implementation Plan — liquers-py-web-server

## Overview

Commit group B on branch `claude/project-thread-5j5pvg` (PR #264), after group A
(`configuration-document` Step 1, `axum-server-assembly` Steps 1–6). It interleaves with
`configuration-document` Steps 2–4, which change the same `Environment::new` and `EnvCell`. No
issue must be fixed first.

## Progress

- [ ] Step 1: the `liquers-axum` dependency
- [ ] Step 2: `EnvCell`'s current build and server list
- [ ] Step 3: `server.rs`: `CellSource`, `ServerSlot`, `stop_all`, `lq.Server`
- [ ] Step 4: `serve`, `serve_async`, `servers`, shutdown and exit order
- [ ] Step 5: the Python surface and stubs
- [ ] Step 6: tests
- [ ] Step 7: documentation, module docs and the wheel size

## Implementation Steps

### Step 1: the `liquers-axum` dependency

- Files: `liquers-py/Cargo.toml` (`liquers-axum = { path = "../liquers-axum" }`).
- Depends on: `axum-server-assembly` Step 4.
- Proof: `cargo check -p liquers-py`.
- Rollback: revert.

### Step 2: `EnvCell`'s current build and server list

- Files / symbols: `liquers-py/src/environment.rs` — `EnvCell::current`, `servers`,
  `current_without_gil`; `retire(cell, declarations)` (line 432); `envref` sets `current` on both
  paths under the declarations lock; `shutdown` clears it.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-py --no-default-features --lib` (existing tests green).
- Rollback: revert.

### Step 3: `server.rs`

- Files / symbols: new `liquers-py/src/server.rs` — `CellSource` (single-flight rebuild through
  `exit::attach_tracked`, refused once `exit::exiting()`), `ServerSlot`, `SERVERS`, `stop_all`,
  `Server` (`address`, `port`, `url`, `running`, `shutdown`, `shutdown_async`, `__enter__`,
  `__exit__`, `__repr__`); `lib.rs` registers `Server`.
- Depends on: Step 2.
- Proof: `cargo test -p liquers-py --no-default-features --lib server` —
  `cell_source_rebuilds_once_after_invalidate`.
- Rollback: revert.

### Step 4: `serve`, `serve_async`, `servers`, shutdown and exit order

- Files / symbols: `environment.rs` — `Environment::serve`, `serve_async`, `servers`;
  `EnvCell::shutdown` stops servers before `shut.fire()`; `exit.rs` `at_exit` and `aio.rs`
  `StopAtExit` call `server::stop_all`, and `serve` registers an `atexit` hook once.
- Change: Phase 2 Interfaces and Integration Points; overrides applied to `PyConfig.server`;
  `check_blocking_call` on foreground `serve` and `shutdown`.
- Depends on: Step 3; `configuration-document` Step 2.
- Proof: Step 6's AC-1–AC-6 tests.
- Rollback: revert; Steps 1–3 stay unused.

### Step 5: the Python surface and stubs

- Files: `python/liquers/_env.py` (`serve`, `serve_async`), `__init__.py` (export `Server`,
  `serve`, `serve_async`), `_native.pyi`, `__init__.pyi`.
- Depends on: Step 4.
- Proof: the stub check of `scripts/test-liquers-py.sh` (the suite's stub test) and
  `python3 -c "import liquers; liquers.Server"` in the venv.
- Rollback: revert.

### Step 6: tests

- Files: new `liquers-py/tests/test_WEBSERV_server.py` (Phase 3 Test Plan).
- Depends on: Steps 4–5; `configuration-document` Step 4 shares the fixtures.
- Proof: `bash scripts/test-liquers-py.sh`; the same file on 3.14t in CI.
- Rollback: revert with the steps it covers.

### Step 7: documentation, module docs and the wheel size

- Files: `specs/reference/PYTHON_INTEGRATION.md` (§Web server; History, `reviewed:`),
  `specs/guides/PYTHON_GUIDE.md` ("Serve an environment over HTTP"), `liquers-py/README.md`
  (`WEBSERV01`–`05` rows and deviations, `06`–`08` `NS`), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`
  §6 Python, `docs/guides/python/12-serving.md` (with a security section naming `host`,
  `read_only`, `web` and the stored-page risk); module docs of `server.rs`, `environment.rs`.
- Also: build the wheel profile (`maturin build --profile wheel --features abi3`) and record the
  size change in the Phase 5 summary.
- Depends on: Steps 1–6.
- Proof: `python3 scripts/check_api_docs.py liquers-py/src/server.rs liquers-py/src/environment.rs`;
  `cargo run -p liquers-docs -- check`; `python3 scripts/docs_index.py --check`.
- Rollback: revert.

## Testing Plan

Rust unit tests after Steps 2–3; the Python suite after Step 6 on 3.13 or 3.14 locally, and CI's
3.10, 3.14 and 3.14t jobs on push. No `cfg(feature)` change.

## Rollback Plan

Group B reverts as a whole without touching group A; reverting group A requires reverting B first.

## Documentation Updates

As Step 7; `specs/index.csv` regenerated. The security guideline of
`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` is its own design: this design writes the serving
chapter's security section and adds its settings to that issue.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
