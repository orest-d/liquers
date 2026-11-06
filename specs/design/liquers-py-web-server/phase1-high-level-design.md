# Phase 1: High-Level Design — liquers-py-web-server

## Purpose

Let a Python script serve its own environment, Python commands included, over the standard Liquers
HTTP API (`liquers-axum`), in the foreground or in the background: the `WEBSERV` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` and the owner's "server" usage mode. It serves `VISION.md`'s data
services use case ("every reachable query is a REST endpoint") for Python users. Milestone M7, with
[`liquers-py-modules`](../liquers-py-modules/DESIGN.md) and [`liquers-py-ui`](../liquers-py-ui/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written; the questions below wait for the Phase 1 gate.
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API and a new
  dependency of `liquers-py`.
- **Leading issue:** question 3, what a dropped handle does. Source issue
  `PY-CANNOT-START-A-WEB-SERVER` (P2 L, filed with this design).
- **Explanation:** the routes, their builders and graceful shutdown exist in Rust; liquers-py has a
  process-wide Tokio runtime (`liquers-py/src/runtime.rs`) that can run `axum::serve`. What is new
  is the Python surface, the handle and its lifetime, and keeping the server on the environment
  the script sees after a rebuild.
- **Open questions:**
  1. **Proposed resolution: where the call lives.** `env.serve(host="127.0.0.1", port=8080,
     background=False, prefix="/liquer", …)` on `Environment`, and `lq.serve(…)` on the default
     environment, as `lq.evaluate` is. It returns an `lq.Server` (`address`, `port`, `url`,
     `running`, `shutdown(timeout=…)`, context manager). The foreground form returns after the
     server stops.
  2. **Proposed resolution: startup errors raise at the call.** `serve` binds the socket before it
     returns, so a port in use or an unresolvable host raises `lq.LiquersError` from `serve`
     itself, and readiness is "`serve` returned". `wait_ready()` is then unnecessary; the guide's
     sample `ready()` is kept only as a property (`running`).
  3. **Open design: lifetime of a background server.** Recommended: it runs until `shutdown()`,
     the end of a `with` block, the environment's shutdown or interpreter exit, and **not** until
     the handle is garbage-collected, as `threading.Thread` and `subprocess.Popen` behave. A
     notebook cell `env.serve(background=True)` then keeps serving; `env.servers` lists the
     running handles. Alternative: the handle owns it strictly and a collected handle stops the
     server with a `ResourceWarning`.
  4. **Proposed resolution: late registrations reach the server.** The server answers with the
     environment the handle has at request time, so a command registered after `serve` is
     callable over HTTP (AC-7). A late registration rebuilds the environment
     (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`); the server follows the rebuild
     as a `web-ui-use` mount does. Alternative: freeze the environment at `serve`.
  5. **Proposed resolution: routes and safety defaults.** All four APIs (query, store, assets with
     its WebSocket, recipes) under `/liquer`, bound to `127.0.0.1` by default; keyword arguments
     drop an API or change the prefix. No authentication: the docs say to bind to localhost or put
     the server behind a proxy until `access-policy-at-entry-points` lands. A `read_only=True`
     keyword needs a read-only `StoreApiBuilder`, which does not exist; it is left out
     (recommended) or added in its own `liquers-axum` PR.
  6. **Proposed resolution: Python handlers.** `WEBSERV06`–`08` stay `NS`, recorded in the
     conformance matrix and in `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS` (filed with this
     design). A custom endpoint is a command under `/liquer/q`.
  7. **Proposed resolution: async forms.** `await env.serve_async(…)` runs the server until the
     awaiting task is cancelled; `await server.shutdown_async()`. `ASYNCQ` is soft for `WEBSERV`,
     but every other blocking liquers-py call has an awaitable form.
  8. **Implementation detail: a shared server assembly in `liquers-axum`.** The router composition
     and graceful shutdown that `examples/basic_server.rs` writes by hand become one small public
     builder, so liquers-py and later hosts (a CLI server, `WEBAPI`) do not copy it. Its own PR.

## Problem Example

```python
import liquers as lq, urllib.request
env = lq.Environment()

@env.command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

with env.serve(port=0, background=True) as server:
    print(urllib.request.urlopen(f"{server.url}/liquer/q/greet-Ann").read())
```

**Today** `env.serve` raises `AttributeError`. Serving needs a Rust program
(`liquers-axum/examples/basic_server.rs`) that builds its own environment, so `greet` is not
there. **After** the request returns `b"Hello, Ann!"` and the port is released when the block
ends. The query `greet-Ann` validates with `greet` declared (`liquers-validate --command greet`).

## Scope and Acceptance Criteria

- **AC-1** Background start on an ephemeral port
  WHEN `env.serve(port=0, background=True)` is called
  THEN it returns an `lq.Server` whose `port` is the bound, non-zero port, `running` is true, and
  a request to `server.url` is answered (WEBSERV01, problem example)
- **AC-2** The query route uses the script's environment
  WHEN `GET {url}/liquer/q/greet-Ann` reaches a server started from `env`
  THEN the body is the Python command's result, and `env.evaluate` of the same query afterwards
  reuses the same asset (WEBSERV02)
- **AC-3** Startup errors are typed
  WHEN `serve` is given a port already in use, or a host that does not resolve
  THEN it raises `lq.LiquersError` with a core error type, and no server is left running (WEBSERV03)
- **AC-4** Graceful shutdown
  WHEN `server.shutdown()` is called while a request is in flight
  THEN the in-flight request completes, new connections are refused, the port is free afterwards,
  and a request after `timeout` seconds is cut off (WEBSERV04)
- **AC-5** The handle owns the server's lifetime
  WHEN a background server's `with` block ends, `env.shutdown()` runs, or the interpreter exits
  THEN the server stops and the port is released; the handle reports `running == False` (WEBSERV05;
  garbage collection per question 3)
- **AC-6** Foreground serving
  WHEN `env.serve(port=…)` blocks the main thread and the user presses Ctrl-C
  THEN the server shuts down gracefully and `serve` raises `KeyboardInterrupt`
- **AC-7** Late registrations are served
  WHEN a command is registered after `serve`
  THEN a request naming it is answered by it (question 4)
- **AC-8** Routes are configurable
  WHEN `serve` gets `prefix="/api"` and `store=False`
  THEN the query route answers under `/api/q/…` and no store route exists
- **AC-9** Async forms
  WHEN `await env.serve_async(port=0)` runs in a task that is later cancelled, and
  `await server.shutdown_async()` stops a background server
  THEN both stop the server gracefully without blocking the event loop

Non-goals: Python handlers and middleware (`WEBSERV06`–`08`, question 6); TLS (a reverse proxy);
authentication (`access-policy-at-entry-points`); a Python web framework (`WEBAPI`, milestone M8);
one server over several environments.

## Core Interactions

Web/API: the `liquers-axum` builders over `PyEnvironment`. Environment: the handle's current build,
followed across rebuilds; shutdown and exit stop servers. Runtime: the process runtime of
`runtime.rs`. Bindings: `Environment.serve` / `serve_async`, `lq.serve`, `lq.Server`.

## Crate Placement

`liquers-py` gains `liquers-axum` as a dependency (core and store only, so the dependency flow
holds) and a new `src/server.rs`; Python surface in `_env.py`, stubs and `__init__.py`. The
shared assembly (question 8) is in `liquers-axum`, its own PR first.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Web server.
- Guide: a "Serve an environment over HTTP" chapter in `PYTHON_GUIDE.md`.
- Other documents: the `WEBSERV` row of `liquers-py/README.md`; `LANGUAGE-INTEGRATION_GUIDE.md`
  §6 Python; `liquers-axum/README.md` for the shared assembly; `specs/README.md`.
- Documents to update: the user guide chapters `docs_index.py chapters-for` lists for the above.
- API docs: the module doc of the new `liquers-py/src/server.rs` and of the `liquers-axum` module
  that gains the assembly; docstrings on every new Python name.

## Design Dependencies

- `language-late-registration-without-rebuild` (in review): **overlaps** AC-7. If it lands first,
  there is no rebuild to follow.
- `access-policy-at-entry-points`, `http-request-body-limit` (in review): **overlap**; the server
  passes their settings on once they exist.
- M6 (PRs #257, #259–#263, open): no dependency. `liquers-py-ui` **requires** this design for its
  "UI with a background server" criterion.

## References

- Guide §WEBSERV and Appendix A `WEBSERV`; `reference/WEB_API_SPECIFICATION.md`
- `liquers-axum/examples/basic_server.rs`; `liquers-py/src/runtime.rs`, `exit.rs`
- `specs/design/web-ui-use/` (following a rebuild, AC-9 there)
