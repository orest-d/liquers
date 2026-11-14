---
id: PY-CANNOT-START-A-WEB-SERVER
kind: feature
title: Python cannot serve its environment over the Liquers HTTP API
status: closed
priority: P2
complexity: L
area: [py, axum]
design: liquers-py-web-server
created: 2026-11-06
github:
---

## Problem

`liquers-py` does not depend on `liquers-axum` and has no server API. The standard routes exist as
builders generic over the environment (`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder`,
`RecipesApiBuilder` in `liquers-axum/src/`), but they can be assembled only from Rust
(`liquers-axum/examples/basic_server.rs`), on a Rust-built environment that has none of the
commands a Python script registered.

**Example.** `env.serve(port=0, background=True)` raises `AttributeError`. Expected: a server
handle whose address answers `GET /liquer/q/greet-Ann` with the Python command's result
(guide Appendix A `WEBSERV01`, `WEBSERV02`).

## Impact

The owner's "server" usage mode of liquers-py (plan §1, decisions of 2026-10-17) is unavailable;
`WEBSERV` is `NS` in `liquers-py/README.md`'s conformance matrix. Workaround: none from Python.

## Expected behaviour

`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §WEBSERV, tests `WEBSERV01`–`05`: foreground and
background start, readiness, typed startup errors, graceful shutdown, a stated lifetime policy
for the server, bound to the same environment as the script.

## Discovery

Milestone M7 of the liquers-py plan (`/mnt/project-files/plans/liquers-py-implementation-plan.md`
§M7), 2026-11-06: no issue recorded that liquers-py does not implement `WEBSERV`.

## Resolution

Closed 2026-11-09 by `liquers-py-web-server` (PR #264). `env.serve()` / `serve_async()` and
`lq.serve()` start liquers-axum's standard server from the configuration's `server` section with
keyword overrides, in the background (an `lq.Server` that survives a dropped handle) or the
foreground; requests use the current build without the GIL, a late registration is served after
one rebuild, and servers stop first at `env.shutdown()` and at interpreter exit. Evidence:
`liquers-py/tests/test_WEBSERV_server.py` (`WEBSERV01`–`05` and the rest). Python request handlers
(`WEBSERV06`–`08`) stay open as `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS`. Reference:
`PYTHON_INTEGRATION.md` §17.
