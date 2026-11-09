# Phase 1: High-Level Design — liquers-py-web-server

## Purpose

Let a Python script serve its own environment, Python commands included, over the standard Liquers
HTTP API (`liquers-axum`), in the foreground or the background: the `WEBSERV` feature of
`LANGUAGE-INTEGRATION_GUIDE.md` and the owner's "server" usage mode, for `VISION.md`'s data services
("every reachable query is a REST endpoint"), including a web application or report site kept in
the store's `web` folder and an index page, as LiQuer's server had. Milestone M7, with
[`liquers-py-ui`](../liquers-py-ui/DESIGN.md); the server itself is
[`axum-server-assembly`](../axum-server-assembly/DESIGN.md).

## Design Readiness

- **Readiness:** `ready` (Phase 2 approved 2026-11-09; Phase 3 approved and Phase 4, implementation and Phase 5
  pre-approved the same day, "proceed all"; `configuration-document` Decision 2 answered the same day, so
  implementation proceeds): Phase 1 written and reviewed twice; Orest decided questions 3
  and 5 and added static content and the index page on 2026-11-06 (Decisions 1-3); approved
  2026-11-07 ("proceed").
- **Automatic fixing:** `not-eligible`: an `L` feature with new public Python API and a new
  dependency of `liquers-py`.
- **Leading issue:** `PY-CANNOT-START-A-WEB-SERVER` (P2 L, filed with this design); the security
  documentation that Decision 1 requires lands with Phase 5.
- **Explanation:** the route builders exist and are generic over the environment; liquers-py has
  a process-wide Tokio runtime (`liquers-py/src/runtime.rs`). Missing are a server assembly that
  can shut down gracefully (nothing in the repository calls `with_graceful_shutdown`, and
  `liquers-axum/examples/basic_server.rs` ends in a bare `axum::serve`), the Python handle and its
  lifetime, and a way to follow liquers-py's environment rebuilds: the builders return
  `Router<EnvRef<E>>`, and `with_state` fixes one `EnvRef`.
- **Open questions:**
  1. **Proposed resolution: the call and its configuration.** The server's settings are part of the
     environment's configuration document (a `server:` section of `lq.Environment(config=…)`:
     host, port, prefix, which APIs, read-only and `web`, the `ServerConfig` of
     `axum-server-assembly`, parsed from the document when the environment is created
     (`PyConfig` of `configuration-document`; Phase 2)), and `env.serve(**overrides)` starts it with
     keyword arguments overriding the section (Decision 1). Defaults: `127.0.0.1:8080`, prefix
     `/liquer`, all four APIs, writes enabled, `background=True` (Decision 2). `lq.serve(…)` serves
     the default environment, as `lq.evaluate` does. It returns an `lq.Server` (`address`, `port`,
     `url`, `running`, `shutdown(timeout=5.0)`, context manager); `background=False` blocks until
     the server stops. The plan's `lq.serve(env, …)` becomes a method plus a default-environment
     function, the liquers-py pattern.
  2. **Proposed resolution: startup errors raise at the call.** `serve` binds the socket before it
     returns, so a port in use or a bad host raises `lq.LiquersError` from `serve`, and readiness
     is "`serve` returned" (the plan's and the guide's `ready()` become the `running` property).
  3. **Decided (Decision 2): a server runs in the background.** `serve` starts it in the
     background by default, and a server whose handle is dropped keeps running until `shutdown()`,
     the end of a `with` block, `env.shutdown()` or interpreter exit; `env.servers` lists the
     running ones. A notebook cell `env.serve()` therefore keeps serving.
  4. **Proposed resolution: late registrations reach the server.** Each request is answered by the
     environment the handle has at that moment, so a command registered after `serve` is
     callable over HTTP (AC-7). The web and index routes follow it like the others. Resolved in Phase 2:
     an `EnvSource` over `EnvCell` (defined by `axum-server-assembly`) gives each request the current
     build. An open WebSocket keeps the build it connected to: its subscriptions keep reporting, and
     a new connection sees the new commands.
  5. **Decided (Decision 1): writes enabled by default, a setting of the configuration, and
     documented as security-sensitive.** Python is "batteries included", so the store and assets
     APIs accept writes unless `read_only: true` (assets read-only through
     `AssetsApiBuilder::read_only`, the store API left out until a read-only store API exists; a
     read-only store mount, `specs/design/web-api-spec-read-only-mount/`, protects part of a store).
     The risk stays real and is documented, not defaulted away: liquers-axum has no authentication
     and no Host or Origin check, so any local client, or a web page through DNS rebinding, can
     write (and, once the deferred `liquers-py-modules` lands, a write under a module prefix is code
     the next import runs). The
     default host is `127.0.0.1`; authentication waits for `access-policy-at-entry-points`. The
     sensitive settings are listed in a new security guideline and in a security chapter of the
     user guides (`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`, filed with this decision, its
     own documentation design); this design adds its settings there.
  6. **Proposed resolution: the server is `axum-server-assembly`, first.** Its own design and PR:
     one builder for the standard APIs, the `web` route over the store's `web/` folder, the index
     page at `/` and `/index.html`, and a shutdown within a deadline. This design exposes it to
     Python; Orest decided there that web content is read from the store only and the index
     page is a fixed redirect (2026-11-06).
  7. **Proposed resolution: Python handlers.** `WEBSERV06`–`08` stay `NS` in the conformance matrix,
     recorded in `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS`. A custom endpoint is a command under
     `/liquer/q`.
  8. **Proposed resolution: conventions shared by the M7 designs.** Every blocking call has an
     `_async` form (`await env.serve_async(…)` serves until its task is cancelled; a background
     server from async code is `env.serve()`, which returns once bound;
     `await server.shutdown_async()`); every `env.` entry point has an `lq.` form on the default
     environment; a blocking call on the main thread ends on Ctrl-C after a graceful stop and raises
     `KeyboardInterrupt`; `env.shutdown()` and interpreter exit stop what the environment started,
     servers before commands are cancelled (`liquers-py/src/exit.rs`).
  9. **Proposed resolution: putting content in the web folder from Python.** A configuration mount
     at `web` (a folder of the host), or `env.store_set("web/index.html", …)` for generated pages;
     no separate Python API. A page stored from Python is text with no media type, so the web route
     takes the type from the key's extension (`axum-server-assembly` question 5). Shipping a built-in Liquers web UI under `web/` in the wheel belongs
     with the UI designs and the wheel-contents decision.
- **Decisions:**
  1. Orest, 2026-11-06, on question 5: "it should be part of the configuration. The philosophy for
     python is 'batteries included by default' so store writes should be enabled. There should be a
     chapter in the user guides about the security discussing what settings are sensitive. There
     should be a security guideline and the user documentation should have a section(s) dealing
     with security implications and security sensitive settings."
  2. Orest, 2026-11-06, on question 3: "ok, server should be running in background".
  3. Orest, 2026-11-06: modules support is split out of M7 into its own design and PR (#265).
     "There are (at least) two needs: serving static content (similarly as the web folder in
     https://github.com/orest-d/liquer which exposes a predefined store) and serving /index.html."
     Added as AC-10 and AC-11 and question 9.
     The web route's risks (pages stored over HTTP run on the API's origin) are named in
     `axum-server-assembly` question 6.
  4. Orest, 2026-11-07, at the Phase 2 review: "there should be a single configuration file (yaml or
     json) that would configure all aspects - server, store, environment, recipe providers, command
     metadata registry (a separate file)... a single config struct assembled from specific config
     structs." Phase 2 should show Rust snippets and a Python example; Phase 3 adds examples without
     a server, of configuring the server in code, a minimal file with every default described,
     and specialised configurations. Phase 2 §Configuration Document answers it.
- **Deviations from Appendix A** (copied to the conformance matrix): `WEBSERV01`'s `ready()` and
  `WEBSERV04`'s `is_running()` are the `running` property; `shutdown(graceful=True)` is
  `shutdown(timeout=…)`, always graceful until the timeout; `inflight_requests()` is not offered
  (the test observes a slow request completing); `WEBSERV05` follows question 3.
- **Decision log (pre-approval, 2026-11-09):**
  - *Needs decision:* the keyword rule (`configuration-document` Decision 2), asked on a card;
    the recommended "keyword wins" is assumed until answered.
  - *Assumption:* all three server designs are implemented on branch `claude/project-thread-5j5pvg`
    (PR #264) in two commit groups (core and liquers-axum, then liquers-py), splittable later.
  - *Assumption:* the security guideline (`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`) stays
    its own design; these designs write the serving chapters' security sections.

## Problem Example

```python
import liquers as lq, urllib.request
env = lq.Environment()

@env.first_command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

with env.serve(port=0) as server:
    print(urllib.request.urlopen(f"{server.url}/liquer/q/greet-Ann").read())
```

**Today** `env.serve` raises `AttributeError`; serving needs a Rust program that builds its own
environment, without `greet`. **After** it prints `b"Hello, Ann!"`, and the port is released when
the block ends. `greet-Ann` validates with `greet` declared (`liquers-validate --command greet`).

## Scope and Acceptance Criteria

- **AC-1** Background start on an ephemeral port
  WHEN `env.serve(port=0)` is called
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
  collected THEN the server keeps running and is listed in `env.servers` (WEBSERV05, Decision 2)
- **AC-6** Foreground serving (addition)
  WHEN `env.serve(port=…, background=False)` blocks the main thread and the user presses Ctrl-C
  THEN the server stops gracefully and `serve` raises `KeyboardInterrupt`
- **AC-7** Late registrations are served (addition)
  WHEN a command is registered after `serve`
  THEN a request naming it is answered by it
- **AC-8** Settings come from the configuration (addition, Decision 1)
  WHEN the configuration's `server:` section sets `prefix: /api` and `read_only: true`, and
  `serve(prefix="/v1")` is called
  THEN the query route answers under `/v1/q/…`, and a store write or an assets mutation is refused;
  with no settings, a store write succeeds
- **AC-9** Async forms (addition)
  WHEN `await env.serve_async(port=0)` runs in a task that is then cancelled, and
  `await server.shutdown_async()` stops a background server
  THEN each stops the server gracefully without blocking the event loop
- **AC-10** A web application from the store (addition, Decision 3)
  WHEN the configuration mounts a folder holding `index.html` and `app.js` at `web`, and the server
  runs
  THEN `GET {url}/liquer/web/` returns `index.html` as `text/html` and `…/web/app.js` the script
- **AC-11** The index page (addition, Decision 3)
  WHEN a browser opens `{url}/` or `{url}/index.html`
  THEN it is redirected to the web folder and gets its `index.html` (`axum-server-assembly` Decision 2)

Non-goals: Python handlers and middleware (question 7); TLS; authentication; a Python web framework
(`WEBAPI`, milestone M8); one server over several environments.

## Core Interactions

Web/API: the `axum-server-assembly` server over `PyEnvironment`, with its web folder and index. Environment: the current build per request; shutdown
and exit stop servers. Bindings: `Environment.serve`, `lq.serve`, `lq.Server`.

## Crate Placement

`liquers-axum`: `axum-server-assembly` (question 6), landing before it in PR #264 (commit group A). `liquers-py`: `liquers-axum` as a
dependency (it depends on core and store only, so the flow holds; liquers-py's own `tokio` gains
`net`), a new `src/server.rs`, the Python surface in `_env.py`, the stubs and `__init__.py`.

## Documentation Intent

- Reference: `PYTHON_INTEGRATION.md`, a new §Web server.
- Guide: a "Serve an environment over HTTP" chapter in `PYTHON_GUIDE.md`; `WEB_API_GUIDE.md` gains
  the shared assembly.
- Other documents: the `WEBSERV` row and deviations in `liquers-py/README.md`;
  `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python; `liquers-axum/README.md`; `specs/README.md`.
- Documents to update: user guide chapter `docs/guides/python/12-serving.md`, and the chapters
  `docs_index.py chapters-for` lists; the server settings join the security guideline and the
  guides' security chapter (Decision 1).
- API docs: the module docs of the new `liquers-py/src/server.rs` and of the `liquers-axum` module
  that gains the assembly; docstrings on every new Python name.

## Design Dependencies

- `configuration-document` (2026-11-07, Decision 4): **requires**; the `server` section is read from it.
- `axum-server-assembly` (`AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN`,
  `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT`): **requires** (question 6).
- `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`: **requires** (Decision 1); its documentation
  design lands before or with this design's Phase 5.
- `language-late-registration-without-rebuild`, `post-init-command-registration` (in review):
  **overlap** question 4; either landing first removes the rebuild to follow.
- `liquers-py-modules` (deferred, PR #265): **overlaps** question 5 (code planted over HTTP).
- `access-policy-at-entry-points`, `http-request-body-limit` (in review): **overlap**; their
  settings are passed on once they exist.
- `liquers-py-ui`: **required-by**, for its GUI-plus-server criterion. M6 (PRs #257, #259–#263): none.

## References

Guide §WEBSERV and Appendix A; `reference/WEB_API_SPECIFICATION.md`; `liquers-axum/src/*/builder.rs`;
`liquers-py/src/runtime.rs`, `environment.rs`, `exit.rs`; `liquers-lib/src/ui/web/app.rs` `mount_web`.
