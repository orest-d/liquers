# Phase 2: Solution & Architecture — liquers-py-web-server

## Overview

A new `liquers-py/src/server.rs` binds an `axum-server-assembly` `LiquersServer` on liquers-py's
process runtime, configured from the `server` section of the document the environment was created
with plus keyword overrides, and answers each request from the environment's current build through
an `EnvSource` over `EnvCell` (question 4). `lq.Server` is a thin handle; the running server is owned
by a process registry, so it survives a dropped handle (Decision 2) and is stopped by
`env.shutdown()` and the exit hook before commands are cancelled. Rejected: landing
`language-late-registration-without-rebuild` first (in review, larger; the source makes it
optional); a Python-side wrapper around a subprocess (no shared environment); taking the GIL on every
request to call `EnvCell::envref` (serializes the server on Python).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-CANNOT-START-A-WEB-SERVER` | draft | P2 | the source issue | — | no | this design |
| `AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN`, `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT` | draft | P2 | the server itself | yes | no (designed around) | `axum-server-assembly` lands first |
| `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` | draft | P2 | Decision 1 needs it | no | no | its docs design lands with Phase 5 |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | no authentication | no | no | `127.0.0.1` default, documented |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | bind errors are `General` | no | no | none now |
| `KEY-NAMES-CANNOT-HOLD-COMMON-WEB-FILENAMES` | draft | P3 | some bundles cannot be stored (question 9) | no | no | documented |
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | a registration rebuilds; WebSocket clients keep the old build | no | no | outcome documented below |
| `ASSET-MANAGER-GET-PRINTS-DEBUG-OUTPUT` | draft | P3 | stderr noise per request | no | no | its own PR |
| `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` | draft | P3 | one file for every section | yes | no | design `configuration-document` lands first |
| `PY-WEB-SERVER-CANNOT-TAKE-PYTHON-HANDLERS` | draft | P3 | `WEBSERV06`–`08` stay `NS` | no | no | M8 |

## Interfaces

```rust
// liquers-py/src/environment.rs — changed
pub struct EnvCell {
    /* …existing fields… */
    /// The whole configuration (see §Configuration Document), parsed at construction, so an
    /// unknown section or key fails at `lq.Environment`.
    pub config: PyConfig,                                  // replaces `config: EnvironmentConfig`
    /// The built environment, readable without the GIL. Written only while the declarations lock is
    /// held: set by `envref` (built and cached paths), cleared by `retire` and `shutdown`.
    current: std::sync::RwLock<Option<EnvRef<PyEnvironment>>>,
    /// Servers started from this environment; stopped ones are pruned on each access.
    pub servers: Mutex<Vec<Arc<ServerSlot>>>,
}
impl EnvCell { pub fn current_without_gil(&self) -> Result<Option<EnvRef<PyEnvironment>>, Error>; }
fn retire(cell: &EnvCell, declarations: &mut Declarations) -> Result<(), Error>; // was (&mut Declarations)

// liquers-py/src/server.rs — new
struct CellSource { cell: Arc<EnvCell>, rebuild: tokio::sync::Mutex<()> }
#[async_trait]
impl EnvSource<PyEnvironment> for CellSource {
    /// `current_without_gil`, else (after a registration) one rebuild at a time: under `rebuild`,
    /// `spawn_blocking(|| exit::attach_tracked(|py| cell.envref(py)))`. Refused once exiting.
    async fn current(&self) -> Result<EnvRef<PyEnvironment>, Error>;
}
pub(crate) struct ServerSlot { server: LiquersServer, prefix: String }
static SERVERS: Mutex<Vec<Weak<ServerSlot>>>;            // every live server, for exit
/// Stops every server with the GIL released (`py.detach`); idempotent.
pub(crate) fn stop_all(py: Python<'_>, deadline: Duration);

#[pyclass(name = "Server", module = "liquers", frozen)]
pub struct Server { slot: Arc<ServerSlot> }
#[pymethods] impl Server {
    #[getter] fn address(&self) -> String;  #[getter] fn port(&self) -> u16;
    #[getter] fn url(&self) -> String;      #[getter] fn running(&self) -> bool;
    #[pyo3(signature = (timeout = 5.0))] fn shutdown(&self, py: Python<'_>, timeout: f64) -> PyResult<()>;
    #[pyo3(signature = (timeout = 5.0))] fn shutdown_async<'py>(&self, py: Python<'py>, timeout: f64) -> PyResult<Bound<'py, PyAny>>;
    fn __enter__(slf: Py<Self>) -> Py<Self>;
    fn __exit__(&self, py: Python<'_>, _exc: …) -> PyResult<bool>;   // shutdown(), False
}

// on `Environment` (#[pymethods]) — new
#[pyo3(signature = (*, host=None, port=None, prefix=None, apis=None, read_only=None, web=None, background=true))]
fn serve(&self, py: Python<'_>, host: Option<String>, port: Option<u16>, prefix: Option<String>,
         apis: Option<Vec<String>>, read_only: Option<bool>, web: Option<&Bound<'_, PyAny>>,
         background: bool) -> PyResult<Server>;
fn serve_async<'py>(&self, py: Python<'py>, /* same keywords, no background */) -> PyResult<Bound<'py, PyAny>>;
#[getter] fn servers(&self) -> Vec<Server>;              // running ones only
```

`serve` is sync and binds before returning (question 2). It calls `cell.envref(py)` first, so a
broken configuration raises there, applies the overrides to `config.server`, and binds through
`block_on`. `background=False` then waits on `stopped()` in the same `block_on`, whose signal checks
give Ctrl-C (AC-6). `serve_async`: the outer `future_into_py_with_locals` future holds a guard that
cancels `stop_token()` when asyncio cancels the task (the builder's default deadline applies),
since `awaitable`'s inner task outlives a cancel (AC-9). `Server.shutdown` waits on
`LiquersServer::shutdown(&self)` with the GIL released, holding no lock. `web` takes a bool or a key
string. Python side: `lq.serve` and `lq.serve_async` in `_env.py`, `Server` exported from
`__init__.py`, the stubs. Commands, ExtValue variants: none.

## Configuration Document

One file configures everything (Decision 4). The document, its sections and `PyConfig`, which
assembles `environment`, `commands`, `server` and `python` from it, are the design
[`configuration-document`](../configuration-document/DESIGN.md), split out on 2026-11-07; this
design uses `PyConfig.server` and the keyword-override rule it defines.

## Setup Examples

A Rust host reads the same file as in `axum-server-assembly` §Integration Points.
A Python script with the same file (`config=` takes a `pathlib.Path` as a file; a `str` stays YAML
text and a `dict` a document):

```python
from pathlib import Path
import liquers as lq

env = lq.Environment(config=Path("liquers.yaml"))

@env.command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

server = env.serve()              # background, settings from `server:`
print(server.url)                 # http://127.0.0.1:8080
# … later, or at interpreter exit:
server.shutdown()
```

```yaml
# liquers.yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: ./data}}   # relative to the working directory
    - {type: filesystem, prefix: web, read_only: true, config: {path: ./site}}
commands:
  metadata: [commands.yaml]       # relative to this file
python:
  pickle: false
server:
  port: 8080
```

Keyword arguments override the file: `lq.Environment(config=…, pickle=True)` and
`env.serve(port=0)`. Phase 3 adds examples without a server, of configuring the server only in
code, a minimal file with every default described, and specialised configurations.

## Integration Points

- `liquers-py/Cargo.toml`: `liquers-axum` (path); its tokio `full` already brings `net`.
- `environment.rs`: `EnvCell` holds the `PyConfig` of `configuration-document`. `shutdown` stops the servers (5 s each, a Phase 2 default,
  waiting with `py.detach` and not the shut-aware `block_on`) before `self.shut.fire()`, then
  stops the managers as today.
- Exit: `server::stop_all(py, 1 s)` (a Phase 2 default) runs from an `atexit` hook registered at the
  first `serve`, from `exit::at_exit`, and from the loop pool's `StopAtExit` (`aio.rs:553`),
  whichever comes first, so servers stop before the async-command loops and before commands are
  cancelled.
- `lib.rs` registers `Server`; `aio::check_blocking_call` guards foreground `serve` and `shutdown`
  on loop-pool threads; `liquers-py/README.md` gains the `WEBSERV` rows.
- `liquers-py/tests/test_server.py`: AC-1..AC-11 with `urllib` and `port=0`.

## Concurrency

The server's tasks run on liquers-py's runtime and never hold the GIL; a Python command a request
reaches runs on a worker as from `evaluate`. The rebuild fallback attaches on a blocking thread, so
the accept loop is never blocked by Python, and `rebuild` lets a burst of requests after a
registration use one blocking thread and one build. After a registration, an open
WebSocket keeps the build it upgraded with: its subscriptions still report, but new commands need
a new connection (`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`); the retired build
stays alive until `shutdown`, as today. `shutdown` from any thread works; on a loop-pool thread the
blocking form raises, as every blocking call there does.

## Error Handling

| Failure | Constructor → Python |
|---|---|
| port in use, unresolvable host | axum's `Error::from_error(ErrorType::General, …)` → `lq.LiquersError` |
| unknown key in `server:` (at `lq.Environment`, `configuration-document`); bad prefix or API name (at `serve`) | `Error::configuration_error(…)` → `lq.ConfigurationError` |
| `serve` after `env.shutdown()` | existing `shut_down_error()` |
| a rebuild fails during a request | 503 with the error detail; the next request retries |
| Ctrl-C during foreground `serve` | graceful stop, then the `KeyboardInterrupt` is re-raised |

## Relevant Commands

None; no namespace is involved. Python commands are reached through `{prefix}/q/…`.

## Documentation Architecture

| Path | Kind | Audience | Change |
|---|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | integrators | new §Web server: settings, lifetime, rebuilds |
| `specs/guides/PYTHON_GUIDE.md` | guide | Python users | "Serve an environment over HTTP" |
| `liquers-py/README.md` | readme | users | `WEBSERV` rows and deviations |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | integrators | §6 Python: WEBSERV status |
| `docs/guides/python/12-serving.md` | user guide | users | serving, web folder, security notes |
| `liquers-py/src/server.rs` | API doc | developers | all five parts |
| `liquers-py/src/environment.rs` | API doc | developers | components gain `current`, `servers` |
| `_env.py`, `__init__.py`, `_native.pyi`, `__init__.pyi` | docstrings | Python users | every new name |
| `specs/README.md` | index | all | link to this design (exists) |

`affects_docs` lists `docs/guides/python/12-serving.md`. The security settings (`host`, `read_only`,
`web`) go to the guideline of `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `server.rs` (new), `environment.rs`, `exit.rs`, `lib.rs`, `_env.py`, `__init__.py`, the stubs, `Cargo.toml` |
| Crates and workflows affected | `liquers-py`; its CI job and the release workflow (wheel grows by axum and hyper: measured in Phase 4) |
| Existing tests likely to change | none; `EnvCell::new` call sites gain the server settings. Recovery: revert the PR |
| New validation | `test_server.py`; free-threaded run (3.14t) of the same file |
| Compatibility / data / concurrency / performance / security | additive; a server's port is held until shutdown or exit; writes enabled by default (Decision 1), on `127.0.0.1` |
| Certainty and open questions | high; medium for the exit ordering on CPython < 3.14, covered by the existing exit tests plus one with a running server |
