# Phase 3: Examples & Use-cases — liquers-py-web-server

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Serve a script's commands | the problem example, `with` block | AC-1, AC-2, AC-5 |
| 2 | Example | The server configured in code | keywords only, no file | AC-1, AC-8 |
| 3 | Example | The server from one file | `server:` section plus overrides | AC-8 |
| 4 | Example | A web page from Python | `store_set("web/…")`, `/` redirect | AC-10, AC-11 |
| 5 | Example | A notebook or a long-running script | background, dropped handle, late registration | AC-5, AC-7 |
| 6 | Example | Foreground and async | `background=False` with Ctrl-C; `serve_async` | AC-6, AC-9 |
| 7 | Test | `liquers-py/tests/test_WEBSERV_server.py` | every scenario | AC-1–AC-11 |

Using an environment without a server, the minimal file and its defaults, and specialised
configurations are `configuration-document` Phase 3, Examples 1–3.

## Example 1: serve a script's commands

```python
import liquers as lq, urllib.request
env = lq.Environment()

@env.command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

with env.serve(port=0) as server:                     # background, an ephemeral port
    print(server.port, server.running)                # e.g. 49152 True
    print(urllib.request.urlopen(f"{server.url}/liquer/q/greet-Ann").read())   # b'Hello, Ann!'
print(server.running)                                 # False: the port is released
print(env.evaluate("greet-Ann"))                      # reuses the asset the request computed
```

`greet-Ann` validates with `liquers-validate --command greet -- greet-Ann`.

## Example 2: the server configured in code

No configuration file; every setting is a keyword, overriding the defaults of
`configuration-document` Example 1:

```python
server = env.serve(host="0.0.0.0", port=8000, prefix="/api",
                   apis=["query", "assets"], read_only=True, web=False)
# GET /api/q/greet-Ann answers; /api/store/… does not exist; POST /api/assets/… is refused;
# neither / nor /api/web/ exists
```

## Example 3: the server from one file

```yaml
# liquers.yaml
store:
  stores:
    - {type: filesystem, prefix: web, read_only: true, config: {path: ./site}}
server: {port: 8000, prefix: /api, read_only: true}
```

```python
from pathlib import Path
env = lq.Environment(config=Path("liquers.yaml"))
server = env.serve(prefix="/v1")    # the file's port 8000 and read_only, prefix /v1 (AC-8)
```

## Example 4: a web page from Python

```python
env.store_set("web/index.html", "<h1>Reports</h1><a href='report.html'>today</a>")
env.store_set("web/report.html", "<p>rows: 3</p>")
server = env.serve(port=0)
# GET {server.url}/            → 307 to /liquer/web/
# GET {server.url}/liquer/web/ → 200 text/html (by the .html extension), the page above
```

A stored page has no media type, so the type comes from the key's extension (Phase 1 question 9).
Pages stored over the HTTP API run on the same origin: the security guideline shows the read-only
`web` mount of Example 3.

## Example 5: a notebook or a long-running script

```python
env.serve(port=8080)               # handle dropped: keeps serving (Decision 2)
env.servers                        # [<liquers.Server http://127.0.0.1:8080 running>]

@env.command                       # registered after serve
def report() -> str:
    return "rows: 3"
# GET /liquer/q/report answers at once (AC-7); env.shutdown() or exit stops the server
```

## Example 6: foreground and async

```python
env.serve(port=8080, background=False)   # blocks; Ctrl-C stops it and raises KeyboardInterrupt
```

```python
async def main():
    server = await env.serve_async(port=0)       # returns when bound
    ...
    await server.shutdown_async()
```

`await env.serve_async(…)` inside a task that is cancelled stops the server through the guard of
Phase 2 (AC-9).

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| port in use | `lq.LiquersError` from `serve`, nothing left listening | `test_WEBSERV03_port_in_use_raises` |
| host that does not resolve | `lq.LiquersError` | `test_WEBSERV03_bad_host_raises` |
| unknown API name, bad prefix | `lq.ConfigurationError` at `serve` | `test_WEBSERV_bad_settings_raise` |
| `serve` after `env.shutdown()` | the shut-down error | `test_WEBSERV_serve_after_shutdown` |
| a registration whose rebuild fails | that request gets 503; the next request retries | `test_WEBSERV_failed_rebuild_is_503` |
| blocking `shutdown` on a loop-pool thread | raises, as other blocking calls do | `test_WEBSERV_shutdown_on_loop_thread_raises` |
| exit with a server running and a command in flight | the server stops first; exit completes | `test_WEBSERV05_exit_stops_server` (subprocess) |
| free-threaded interpreter | the whole file passes on 3.14t | CI job |

## Test Plan

- Python: `liquers-py/tests/test_WEBSERV_server.py`, with `urllib.request` and `port=0` throughout
  - `test_WEBSERV01_start_on_ephemeral_port_and_reach_readiness` (AC-1)
  - `test_WEBSERV02_standard_route_uses_configured_environment` (AC-2): body and reuse
  - `test_WEBSERV03_port_in_use_raises`, `test_WEBSERV03_bad_host_raises` (AC-3)
  - `test_WEBSERV04_graceful_shutdown` (AC-4): a command waiting on a `threading.Event`; with
    the event set it completes, without it it is cut off at the timeout; then the port is free
  - `test_WEBSERV05_background_handle_owns_server_lifetime` (AC-5): `with`, `env.shutdown()`,
    dropped handle listed in `env.servers`; `test_WEBSERV05_exit_stops_server` in a subprocess
  - `test_WEBSERV_foreground_ctrl_c` (AC-6): a subprocess sent `SIGINT` exits with
    `KeyboardInterrupt` after the server stopped
  - `test_WEBSERV_late_registration_is_served` (AC-7)
  - `test_WEBSERV_settings_from_the_configuration` (AC-8): Example 3 with `port: 0`
  - `test_WEBSERV_async_forms` (AC-9): `serve_async` cancelled, `shutdown_async`
  - `test_WEBSERV_web_folder_from_a_mount` (AC-10): a `tmp_path` mount at `web`
  - `test_WEBSERV_index_redirects_to_web` (AC-11): `/` and `/index.html` with redirects followed
- Rust: `liquers-py/src/server.rs` `tests` — `cell_source_rebuilds_once_after_invalidate` (AC-7).
- Command: `bash scripts/test-liquers-py.sh` (venv, `maturin develop`, pytest, the Rust tests).

## Learning Log

- The guide's sample tests (`LANGUAGE-INTEGRATION_GUIDE.md` §WEBSERV) take `env, srv` fixtures;
  here every test starts its own server with `port=0`, so they run in parallel without collisions.
