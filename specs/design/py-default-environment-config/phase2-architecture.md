# Phase 2: Solution & Architecture — py-default-environment-config

## Overview

The settings move into `EnvCell`'s declarations as one replaceable `Settings` value, under the lock
that already serializes registration and building. A Python command no longer copies conversion and
pickle when it is registered: a build binds them when it registers the command. Because `configure`
is refused once a build has succeeded (Phase 1, question 1), every later build sees the same
settings. `Environment(...)` and `configure(...)` resolve their keywords through one function.

Rejected alternatives:
- callables that read the cell on each call: a lock per call, for values that cannot change after
  the build;
- a settings singleton: that is `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`;
- rebuilding on `configure`: decided against (question 1).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION` | draft | P3 | late registrations still rebuild; unchanged | no | no | none |
| `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON` | draft | P2 | the defect AC-7 avoids | no | no | cite |
| `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` | draft | P2 | `pickle` can now be set late | no | no | keep the warnings |
| `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` | draft | P3 | `_env.py`'s docstring is brought up to §3 here | no | no | cite |

Nothing blocks. No command namespace is involved, since the design adds no command.

## Interfaces

`environment.rs`:

```rust
/// Everything `Environment(...)` and `configure(...)` set: keyword > file > default.
#[derive(Default)]                     // the defaults of `Environment()`; used by tests
pub struct Settings {
    pub config: PyConfig,
    pub conversion: Conversion,
    pub async_loops: NonZeroUsize,
    /// The `command_registry:` file's commands, read by `resolve` (AC-6).
    pub registry_file: Vec<CommandMetadata>,
}

impl Settings {
    /// Parses `config`, applies the keywords and reads the registry file. It runs before any
    /// lock is taken.
    pub fn resolve(py: Python<'_>, config: Option<&Bound<'_, PyAny>>, conversion: Option<&str>,
                   pickle: Option<bool>, async_loops: Option<i64>, job_capacity: Option<i64>)
                   -> Result<Settings, Error>;
}

struct Declarations {
    // … existing fields; `imported` now holds only `import_metadata`'s entries
    settings: Arc<Settings>,
}

impl EnvCell {
    pub fn new(settings: Settings) -> Self;            // was (config, conversion, pickle, loops) -> Result
    /// The build and its settings, taken under one lock.
    pub fn envref_and_settings(&self, py: Python<'_>) -> Result<(EnvRef<PyEnvironment>, Arc<Settings>), Error>;
    pub fn configure(&self, py: Python<'_>, settings: Settings) -> Result<(), Error>;
}

#[pymethods]
impl Environment {
    #[pyo3(signature = (*, config=None, conversion=None, pickle=None, async_loops=None, job_capacity=None))]
    fn configure(&self, py: Python<'_>, config: Option<&Bound<'_, PyAny>>, conversion: Option<&str>,
                 pickle: Option<bool>, async_loops: Option<i64>, job_capacity: Option<i64>) -> PyResult<()>;
}
```

"Built" is `declarations.store.is_some()`. It is set only by a successful build, left alone by
`retire` and cleared only by `shutdown`, so no new flag is needed. `configure` runs these steps in
order, under the lock, so that a failure changes nothing:

1. refuse after shutdown;
2. refuse once built;
3. when `declared` is not empty, run the trial build and refuse only issues about declared commands
   (other issues wait for the build, as with `Environment(...)`);
4. `LazyLoopPool::resize`;
5. assign the settings.

`EnvCell`'s `config`, `conversion` and `pickle` fields are removed. All of this is synchronous.

`liquers-py/src/value.rs` and `command.rs`:

```rust
#[derive(Clone, Copy, Debug, Default)]
pub struct Conversion { pub mode: ConversionMode, pub pickle: bool }

pub fn register_py_command(registry: &mut CommandRegistry<PyEnvironment>, spec: &PyCommandSpec,
                           conversion: Conversion) -> Result<Replaced, Error>;
```

`PyCallable` and `PyCommandSpec::new` lose `conversion` and `pickle`. The registered closure
captures `(Arc<PyCallable>, Conversion)` and passes it through `call_py_command`, `call_with_gil`,
`coroutine_result`, `prepare_call` and `start_coroutine`. `Context::with_conversion` then takes a
`Conversion`.

`liquers-py/src/aio.rs`: the size moves into the slot (`PoolSlot::Idle(NonZeroUsize)`).

```rust
impl LazyLoopPool {
    /// Idle: sets the size. Running with another size: ConfigurationError. Closed: shut down.
    pub(crate) fn resize(&self, py: Python<'_>, size: NonZeroUsize) -> Result<(), Error>;
}
```

A failed build that ran an async Python store or recipe method can already have started the pool.
After such a build (AC-9), `configure` may keep the same `async_loops` but cannot change it.

`liquers-py/python/liquers/_env.py`:

```python
def configure(*, config: Any = None, conversion: str | None = None, pickle: bool | None = None,
              async_loops: int | None = None, job_capacity: int | None = None) -> None: ...
```

## Integration Points

- `environment.rs`:
  - `Environment::new` becomes `Settings::resolve` + `EnvCell::new`.
  - `configured_builder` and `build` read `declarations.settings`. `apply_imported` runs over
    `settings.registry_file` and then `imported`, so `import_metadata` wins (AC-3).
  - `_register_command` drops the conversion arguments.
  - `server_config`, `store_set` / `store_set_async` (`stored_state`) and `render_top` /
    `render_top_async` (`build_request`) call `envref_and_settings` first. Today `stored_state` and
    `build_request` run before the build, so a bad argument there will now raise after the build.
- `command.rs`, `context.rs` and `aio.rs`: as above.
- `_env.py`, `__init__.py` (`__all__`) and `__init__.pyi`: `configure` and `Environment.configure`.
- The test call sites `EnvCell::new(...)?` in `environment.rs` and `server.rs` tests and in
  `records/resolver.rs` become `EnvCell::new(Settings::default())`.
- Unchanged: `config.rs`, the `store/` and `recipe_provider.rs` factories, and `CellSource`.
  Nothing else converts with these settings.

## Error Handling

| Case | Error |
|---|---|
| `configure` after a successful build (AC-4) | `Error::configuration_error("the environment is already built: configure must be called before its first evaluation, serve, ui or store write")` |
| `configure` after `shutdown()` | the existing `shut_down_error()` |
| A bad document or keyword (AC-6) | the existing errors of `PyConfig::from_python`, raised by `resolve` |
| A missing or invalid registry file (AC-6) | `read_command_registry`. A missing file becomes `Error::configuration_error`; it is `general_error` today |
| A declared command fails the trial | `Error::configuration_error` with the trial's summary (question 3: checked at the call) |
| `async_loops` changed after the pool started | `Error::configuration_error("async loops already started with N loops")` |

## Relevant Commands

None.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `docs/guides/python/08-environment.md` | user guide | §8.1 `lq.configure` with commands registered at import; §8.2 `env.configure` and the rule "until the first build" |
| `docs/guides/python/02-installing.md`, `12-serving.md` | user guide | the default-environment note and §12.3 point to §8.1 |
| `specs/guides/PYTHON_GUIDE.md` | guide | "The default environment": configuring it |
| `specs/reference/PYTHON_INTEGRATION.md` | reference | §"Default environment", the method table, and the rule that the first successful build fixes the settings |
| `liquers-py/src/environment.rs` module doc | API doc | purpose; the section "Retained declarations, lazy build, rebuild on change" (settings fixed by the first successful build); `Main components` gains `Environment::configure` and `Settings` |
| `liquers-py/python/liquers/_env.py` docstring | API doc | all five parts of `API_DOCUMENTATION_GUIDE.md` §3 (one line today) |
| `liquers-py/python/liquers/__init__.pyi` | stubs | `configure`, `Environment.configure` |

`affects_docs` is as in `DESIGN.md`. Phase 5 adds the design to `specs/README.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `environment.rs`, `command.rs`, `context.rs`, `value.rs`, `aio.rs`, `records/resolver.rs`, `_env.py`, `__init__.py`, `__init__.pyi` |
| Crates and workflows affected | `liquers-py` only; `scripts/test-liquers-py.sh`, CI `liquers-py-tests.yml` |
| Existing tests likely to change | the Rust `EnvCell::new` call sites |
| New validation | Python tests for AC-1 to AC-9; a Rust test for `LazyLoopPool::resize` |
| Compatibility | additive in Python, and `Environment(...)` is unchanged. A missing registry file becomes `ConfigurationError` |
| Concurrency | `configure` and a first build serialize on the declarations lock (declarations, then pool slot, the existing order). `resolve` reads the files before the lock. Registration reads no setting |
