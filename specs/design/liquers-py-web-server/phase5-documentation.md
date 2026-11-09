# Phase 5: Documentation - liquers-py-web-server

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (no PR review comments yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #264)

## Implementation Summary

`env.serve()` serves an environment with liquers-axum's standard server
(`liquers-py/src/server.rs`, `environment.rs`). Settings are the configuration's `server` section,
then the keywords `host`, `port`, `prefix`, `apis`, `read_only`, `web`. `serve` binds before it
returns an `lq.Server` (`address`, `port`, `url`, `running`, `shutdown(timeout=5.0)`,
`shutdown_async`, a context manager); the server runs in the background and survives a dropped
handle (Orest, 2026-11-06), or with `background=False` serves until Ctrl-C. `serve_async()` is a
coroutine that serves until its task is cancelled. `lq.serve` / `lq.serve_async` serve the default
environment; `env.servers` lists the running servers.

Requests are answered from the environment's current build through `CellSource`: `EnvCell` keeps
the last `EnvRef` in a slot read without the GIL, a registration clears it, and the next request
rebuilds once on a blocking thread while the others wait; a failing build answers 503. Servers stop
first: `env.shutdown()` stops its servers (5 s each) before the asset managers, and the exit hooks
(`serve`'s own, the module's, the loop pool's) stop every server (1 s each) before commands are
cancelled. Writes are enabled by default and documented as security-sensitive (Decision 1).

It conforms to Phases 1–4. One correction during testing: `pyo3-async-runtimes` returns an
`asyncio.Future`, which `asyncio.create_task` refuses, so `serve_async` wraps it in a coroutine
(`liquers._env._awaiting`) to keep Phase 3's `asyncio.create_task(env.serve_async())` working.
Wheel size (Step 7), from the pull request's CI: the Linux x86_64 abi3 wheel is 33.2 MB, against
29.6 MB measured on 2026-11-06 before M6 and the server; the 3.6 MB difference includes M6 (merged
in since), so the server's own share is smaller. It adds no linked library (auditwheel lists only
the system ones).

## Documentation Delivered

### New Reference Documents
None: the server is §17 of `PYTHON_INTEGRATION.md`, as planned.

### New Guide Documents
None, as planned.

### Existing Documents Reviewed or Updated
`specs/reference/PYTHON_INTEGRATION.md` (§17 Web server; §1–§3 and §7: `Server`, `liquers-axum`,
`serve`/`servers`, servers stop first), `specs/guides/PYTHON_GUIDE.md` (§6 "Serving over HTTP",
run by `test_GUIDE_examples.py`; §10), `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` (§6 Python:
the web server pattern), `liquers-py/README.md` (`WEBSERV` row: `PARTIAL`, `WEBSERV01`–`05` pass,
`06`–`08` `NS`). History rows and `reviewed: 2026-11-09` on the `specs/` documents.

### Module Docs
`src/server.rs` (new), `src/environment.rs` and `src/aio.rs` (brought up to §3), `src/exit.rs`:
`check_api_docs.py` 0 warnings, 0 errors. `python/liquers/__init__.py` lists `Server`, `serve`,
`serve_async`; the stubs declare them (`serve_async` as a `Coroutine`).

### User-Guide Chapters
`docs/guides/python/12-serving.md` is `draft`: 12.1 starting (background, foreground, asyncio),
12.2 settings with the shared settings, web folder and **Security** fragment, 12.3 stopping;
12.4–12.5 stay `TBD:`. Other chapters listed by `chapters-for`: see `configuration-document`.

### Links and Capability Map
`specs/README.md` keeps its design row until `status: complete`.

## Issues Filed

None new. Closed: `PY-CANNOT-START-A-WEB-SERVER`. Open by decision: `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS`
(`WEBSERV06`–`08`), `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` (the guideline).

## Important Learning

An `lq.Environment` is a frozen native class: tests cannot hang attributes on it. A stopped
server's port can sit in `TIME_WAIT` after the server cut a request off, so the tests check for a
listener with `SO_REUSEADDR`. A stop that runs from `atexit` must not need a tokio context; the
server's `shutdown` only awaits tokens, so `futures::executor::block_on` is enough.

## Conformance and Remaining Work

Implemented as approved; `WEBSERV` is `PARTIAL` because Python handlers were out of scope from
Phase 1. Remaining: the open issues above.

## Validation

`bash scripts/test-liquers-py.sh` on CPython 3.13: 542 passed, 23 skipped after merging main (STUBS needs 3.14 and
runs in CI), plus the crate's Rust unit tests (`cell_source_rebuilds_once_after_invalidate`);
`test_WEBSERV_server.py` and `test_CONFIG_document.py` five times in a row; `check_api_docs.py`;
`cargo run -p liquers-docs -- check`; `python3 scripts/docs_index.py --check`.
