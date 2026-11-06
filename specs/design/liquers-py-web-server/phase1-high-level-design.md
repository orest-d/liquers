# Phase 1: High-Level Design — liquers-py-web-server

## Purpose

Let a Python script serve its own environment, Python commands included, over the standard Liquers
HTTP API (`liquers-axum`), in the foreground or the background: the `WEBSERV` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` and the owner's "server" usage mode, for `VISION.md`'s data services
("every reachable query is a REST endpoint"). Milestone M7, with
[`liquers-py-modules`](../liquers-py-modules/DESIGN.md) and [`liquers-py-ui`](../liquers-py-ui/DESIGN.md).

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written and reviewed twice; questions 3 and 5 wait for the
  Phase 1 gate.
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API and a new
  dependency of `liquers-py`.
- **Leading issue:** question 5, what a server started from Python lets a client write. Source
  issue `PY-CANNOT-START-A-WEB-SERVER` (P2 L, filed with this design).
- **Explanation:** the route builders exist and are generic over the environment; liquers-py has
  a process-wide Tokio runtime (`liquers-py/src/runtime.rs`). Missing are a server assembly that
  can shut down gracefully (nothing in the repository calls `with_graceful_shutdown`, and
  `liquers-axum/examples/basic_server.rs` ends in a bare `axum::serve`), the Python handle and its
  lifetime, and a way to follow liquers-py's environment rebuilds: the builders return
  `Router<EnvRef<E>>`, and `with_state` fixes one `EnvRef`.
- **Open questions:**
  1. **Proposed resolution: the call.** `env.serve(host="127.0.0.1", port=8080, background=False,
     prefix="/liquer", query=True, store=True, assets=True, recipes=True, read_only=…)` and
     `lq.serve(…)` on the default environment, as `lq.evaluate` is. It returns an `lq.Server`
     (`address`, `port`, `url`, `running`, `shutdown(timeout=5.0)`, context manager). The
     foreground form returns after the server stops. The plan's `lq.serve(env, …)` becomes a method
     plus a default-environment function, the liquers-py pattern.
  2. **Proposed resolution: startup errors raise at the call.** `serve` binds the socket before it
     returns, so a port in use or a bad host raises `lq.LiquersError` from `serve`, and readiness
     is "`serve` returned" (the plan's and the guide's `ready()` become the `running` property).
  3. **Open design: a background server whose handle is dropped.** Recommended: it keeps running
     until `shutdown()`, the end of a `with` block, `env.shutdown()` or interpreter exit, and is
     listed in `env.servers`, as `threading.Thread` and `subprocess.Popen` behave; a notebook cell
     `env.serve(background=True)` then keeps serving. Alternative (the guide's first option): a
     collected handle stops the server, with a `ResourceWarning`.
  4. **Proposed resolution: late registrations reach the server.** Each request is answered by the
     environment the handle has at that moment, so a command registered after `serve` is
     callable over HTTP (AC-7). Phase 2 chooses the mechanism: a service that takes the current
     `EnvRef` per request (as `mount_web`'s `current_env` does per tick), or landing
     `language-late-registration-without-rebuild` first. A rebuild starts a new asset manager, so
     WebSocket subscribers of the retired one see no further events; Phase 2 states the outcome.
  5. **Open design (blocking): writes from HTTP clients.** liquers-axum has no authentication and
     no Host or Origin check, and the store API always offers `PUT` and `DELETE`. With
     `liquers-py-modules`, a write under a trusted module prefix is code the next import runs, so a
     local client, or a web page through DNS rebinding, could execute code in the process.
     Recommended: `read_only=True` by default: the assets API read-only
     (`AssetsApiBuilder::read_only`), no store API, recipes as they are (read-only); `read_only=False`
     opts into writes, and the docs point to a read-only store mount
     (`specs/design/web-api-spec-read-only-mount/`) for code prefixes. Bound to `127.0.0.1` by
     default; authentication waits for `access-policy-at-entry-points`. Alternative: everything
     writable by default, as `basic_server.rs`.
  6. **Proposed resolution: a shared server assembly in `liquers-axum`, first.** One public builder
     composes the routers and owns the accept loop, so it can stop accepting, let in-flight
     requests finish, end WebSocket tasks and abort what is left at a deadline; `axum::serve`
     aborted after a timeout does not end connections already spawned. Its own issue
     (`AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN`, filed with this design) and PR; the example and
     later hosts (a CLI server, `WEBAPI`) use it.
  7. **Proposed resolution: Python handlers.** `WEBSERV06`–`08` stay `NS` in the conformance matrix,
     recorded in `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS`. A custom endpoint is a command under
     `/liquer/q`.
  8. **Proposed resolution: conventions shared by the M7 designs.** Every blocking call has an
     `_async` form (`await env.serve_async(…)` serves until its task is cancelled;
     `await server.shutdown_async()`); every `env.` entry point has an `lq.` form on the default
     environment; a blocking call on the main thread ends on Ctrl-C after a graceful stop and raises
     `KeyboardInterrupt`; `env.shutdown()` and interpreter exit stop what the environment started,
     servers before commands are cancelled (`liquers-py/src/exit.rs`).
- **Deviations from Appendix A** (copied to the conformance matrix): `WEBSERV01`'s `ready()` and
  `WEBSERV04`'s `is_running()` are the `running` property; `shutdown(graceful=True)` is
  `shutdown(timeout=…)`, always graceful until the timeout; `inflight_requests()` is not offered
  (the test observes a slow request completing); `WEBSERV05` follows question 3.

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

**Today** `env.serve` raises `AttributeError`; serving needs a Rust program that builds its own
environment, without `greet`. **After** it prints `b"Hello, Ann!"`, and the port is released when
the block ends. `greet-Ann` validates with `greet` declared (`liquers-validate --command greet`).

## Scope and Acceptance Criteria

- **AC-1** Background start on an ephemeral port
  WHEN `env.serve(port=0, background=True)` is called
  THEN it returns an `lq.Server` with the bound non-zero `port` and `running` true, and `server.url`
  answers (WEBSERV01, problem example)
- **AC-2** The query route uses the script's environment
  WHEN `GET {url}/liquer/q/greet-Ann` reaches a server started from `env`
  THEN the body is the Python command's result, and `env.evaluate("greet-Ann")` afterwards reuses
  that asset (WEBSERV02)
- **AC-3** Startup errors are typed
  WHEN `serve` gets a port already in use, or a host that does not resolve
  THEN it raises `lq.LiquersError` with a core error type, and nothing is left listening (WEBSERV03)
- **AC-4** Graceful shutdown
  WHEN `server.shutdown(timeout=t)` is called while a request is in flight
  THEN new connections are refused, the request completes if it finishes within `t` and is cut
  off if it is still in flight when `t` expires, and the port is free afterwards (WEBSERV04)
- **AC-5** The server's lifetime follows its owner
  WHEN a `with` block ends, `env.shutdown()` runs or the interpreter exits
  THEN the server stops, the port is released and `running` is false; WHEN the handle is only
  collected THEN the policy of question 3 holds (WEBSERV05)
- **AC-6** Foreground serving (addition)
  WHEN `env.serve(port=…)` blocks the main thread and the user presses Ctrl-C
  THEN the server stops gracefully and `serve` raises `KeyboardInterrupt`
- **AC-7** Late registrations are served (addition)
  WHEN a command is registered after `serve`
  THEN a request naming it is answered by it
- **AC-8** Routes and writes are configurable (addition)
  WHEN `serve` gets `prefix="/api"`, and separately the defaults of question 5
  THEN the query route answers under `/api/q/…`, and with the defaults a store write or an assets
  mutation is refused
- **AC-9** Async forms (addition)
  WHEN `await env.serve_async(port=0)` runs in a task that is then cancelled, and
  `await server.shutdown_async()` stops a background server
  THEN each stops the server gracefully without blocking the event loop

Non-goals: Python handlers and middleware (question 7); TLS; authentication; a Python web framework
(`WEBAPI`, milestone M8); one server over several environments.

## Core Interactions

Web/API: `liquers-axum` over `PyEnvironment`. Environment: the current build per request; shutdown
and exit stop servers. Bindings: `Environment.serve`, `lq.serve`, `lq.Server`.

## Crate Placement

`liquers-axum`: the shared assembly (question 6), its own PR first. `liquers-py`: `liquers-axum` as a
dependency (it depends on core and store only, so the flow holds; liquers-py's own `tokio` gains
`net`), a new `src/server.rs`, the Python surface in `_env.py`, the stubs and `__init__.py`.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Web server.
- Guide: a "Serve an environment over HTTP" chapter in `PYTHON_GUIDE.md`; `WEB_API_GUIDE.md` gains
  the shared assembly.
- Other documents: the `WEBSERV` row and deviations in `liquers-py/README.md`;
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `liquers-axum/README.md`; `specs/README.md`.
- Documents to update: the user guide chapters `docs_index.py chapters-for` lists.
- API docs: the module docs of the new `liquers-py/src/server.rs` and of the `liquers-axum` module
  that gains the assembly; docstrings on every new Python name.

## Design Dependencies

- `AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN`: **requires** (question 6).
- `language-late-registration-without-rebuild`, `post-init-command-registration` (in review):
  **overlap** question 4; either landing first removes the rebuild to follow.
- `liquers-py-modules`: **overlaps** question 5 (code planted over HTTP).
- `access-policy-at-entry-points`, `http-request-body-limit` (in review): **overlap**; their
  settings are passed on once they exist.
- `liquers-py-ui`: **required-by**, for its GUI-plus-server criterion. M6 (PRs #257, #259–#263): none.

## References

Guide §WEBSERV and Appendix A; `reference/WEB_API_SPECIFICATION.md`; `liquers-axum/src/*/builder.rs`;
`liquers-py/src/runtime.rs`, `environment.rs`, `exit.rs`; `liquers-lib/src/ui/web/app.rs` `mount_web`.
