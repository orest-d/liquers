# Phase 4: Implementation Plan — liquers-py-web-server

## Overview

Commit group B on branch `claude/project-thread-5j5pvg` (PR #264), after group A
(`configuration-document` Step 1, `axum-server-assembly` Steps 1–7). It interleaves with
`configuration-document` Steps 2–4, which change the same `Environment::new` and `EnvCell`. No
issue must be fixed first. Rust unit tests run inside `scripts/test-liquers-py.sh` (`cargo test -p
liquers-py --no-default-features --features debug-handles --lib`, with the venv's interpreter on
`LD_LIBRARY_PATH`), so every proof below that names them runs the script.

## Progress

- [ ] Step 1: the `liquers-axum` dependency
- [ ] Step 2: `EnvCell`'s current build
- [ ] Step 3: `server.rs`: `CellSource`, `ServerSlot`, `stop_all`, `lq.Server`, its stub
- [ ] Step 4: `serve`, `serve_async`, `servers`, shutdown and exit order
- [ ] Step 5: the Python surface and stubs
- [ ] Step 6: tests
- [ ] Step 7: documentation, module docs and the wheel size

## Implementation Steps

### Step 1: the `liquers-axum` dependency

- Files: `liquers-py/Cargo.toml` (`liquers-axum = { path = "../liquers-axum" }`; `tokio` gains
  `net`).
- Depends on: group A (`axum-server-assembly` Step 4 at least).
- Proof: `cargo check -p liquers-py`.
- Rollback: revert.

### Step 2: `EnvCell`'s current build

- Files / symbols: `liquers-py/src/environment.rs` — `EnvCell::current` (a GIL-free `RwLock`) and
  `current_without_gil`; `retire(cell, declarations)` (line 432) and its call sites (lines 176,
  265, 285, 314, 336, 345); `envref` sets `current` on both paths under the declarations lock;
  `shutdown` clears it.
- Depends on: Step 1.
- Proof: `bash scripts/test-liquers-py.sh` (the existing Python suite and Rust unit tests green).
- Rollback: revert.

### Step 3: `server.rs`

- Files / symbols: new `liquers-py/src/server.rs` — `CellSource` (single-flight rebuild through
  `exit::attach_tracked`, refused once `exit::exiting()`), `ServerSlot`, `SERVERS`, `stop_all`,
  `Server` (`address`, `port`, `url`, `running`, `shutdown`, `shutdown_async`, `__enter__`,
  `__exit__`, `__repr__`); `EnvCell::servers` (a list of `ServerSlot`); `lib.rs` registers
  `Server`; `python/liquers/_native.pyi` declares it, so the stub checks stay green.
- Depends on: Step 2.
- Proof: `bash scripts/test-liquers-py.sh` — the Rust test `cell_source_rebuilds_once_after_invalidate`
  and `test_STUBS_declarations.py` (CPython 3.14 with the GIL: `PYTHON=python3.14`).
- Rollback: revert.

### Step 4: `serve`, `serve_async`, `servers`, shutdown and exit order

- Files / symbols: `environment.rs` — `Environment::serve`, `serve_async`, `servers`;
  `EnvCell::shutdown` stops servers before `shut.fire()`; `exit.rs` `at_exit` and `aio.rs`
  `StopAtExit` call `server::stop_all`, and `serve` registers an `atexit` hook once.
- Change: Phase 2 Interfaces and Integration Points; overrides applied to `PyConfig.server`;
  `check_blocking_call` on foreground `serve` and `shutdown`.
- Depends on: Step 3; `configuration-document` Step 2.
- Proof: an interim check in the venv (the native `serve(port=0)` answers
  `GET {url}/q/…` with `urllib`, `shutdown()` frees the port) and the existing suite green; AC-1–AC-11
  are proved by Step 6's tests.
- Rollback: revert; Steps 1–3 stay unused.

### Step 5: the Python surface and stubs

- Files: `python/liquers/_env.py` (`serve`, `serve_async`, `servers`), `__init__.py` (export
  `Server`, add it to `__all__`), `_native.pyi` (the `Environment` methods), `__init__.pyi`.
- Depends on: Step 4.
- Proof: `bash scripts/test-liquers-py.sh` with `PYTHON=python3.14` — `test_STUBS_declarations.py`
  and `test_DOCS_api_reference.py` (which checks `__all__`); `python3 -c "import liquers;
  liquers.Server"` in the venv.
- Rollback: revert.

### Step 6: tests

- Files: new `liquers-py/tests/test_WEBSERV_server.py` (Phase 3 Test Plan). No shared fixture file:
  each test starts its own server with `port=0` from `tmp_path` files, and
  `test_CONFIG_document.py` does the same. `test_WEBSERV_open_websocket_keeps_its_build` uses a
  minimal standard-library WebSocket client in the test file (handshake, masked text frames), so the
  script gains no dependency.
- Depends on: Steps 4–5; `configuration-document` Step 2.
- Proof: `bash scripts/test-liquers-py.sh` (AC-1–AC-11); the same file on 3.10, 3.14 and 3.14t in
  CI.
- Rollback: revert with the steps it covers.

### Step 7: documentation, module docs and the wheel size

- Files: `specs/reference/PYTHON_INTEGRATION.md` (§Web server; History, `reviewed:`),
  `specs/guides/PYTHON_GUIDE.md` ("Serve an environment over HTTP"), `liquers-py/README.md`
  (`WEBSERV01`–`05` rows and deviations, `06`–`08` `NS`), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`
  §6 Python, `docs/guides/python/12-serving.md` (with a security section naming `host`,
  `read_only`, `web` and the stored-page risk); module docs of `server.rs`, `environment.rs`.
- Also: the wheel size change. `maturin build --manifest-path liquers-py/Cargo.toml --profile wheel
  --features abi3` takes about 18 minutes locally (about 30.9 MB before); the figure may instead be
  read from CI's wheel build. Recorded in the Phase 5 summary.
- Depends on: Steps 1–6.
- Proof: `python3 scripts/check_api_docs.py liquers-py/src/server.rs liquers-py/src/environment.rs`;
  `cargo run -p liquers-docs -- check`; `python3 scripts/docs_index.py --check`.
- Rollback: revert.

## Testing Plan

Rust unit tests with the script after Steps 2–3; the Python suite after Step 6 on 3.13 or 3.14
locally (3.14 with the GIL for the stub checks), and CI's 3.10, 3.14 and 3.14t jobs on push. No
`cfg(feature)` change.

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
