# Phase 1: High-Level Design — py-default-environment-config

## Purpose

A program should be able to configure the shared default environment (`lq.evaluate`,
`lq.serve`, `lq.ui`, `@lq.command`) after modules have registered commands. Today it cannot. `lq.default_environment()` is a bare `Environment()` whose settings are fixed when it is
made, and the first `@lq.command` makes it, usually at import. Requiring configuration before any
registration was rejected (Orest, 2026-11-14), because a misplaced import would leave commands in
the wrong environment.

## Problem Example

```python
# app/commands.py
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

# app/main.py
import pathlib
import liquers as lq
import app.commands                     # the default environment exists from here on

lq.configure(config=pathlib.Path("prod.yaml"))   # today: no such function
lq.serve(background=False)              # should serve prod.yaml's store and port, with hello
```

Today `lq.Environment(config=…)` makes a second environment that lacks `hello`, and `lq.serve`
still serves the unconfigured default on port 8080. Expected: one environment, with `hello`,
configured by `prod.yaml`.

## Scope and Acceptance Criteria

"Built" means that a build has succeeded once. A later registration marks the environment for a
rebuild without making it unbuilt again, and a failed build does not count.

- **AC-1** Configure after import-time registration
  WHEN a module registers commands with `@lq.command` and the program then calls
  `lq.configure(config=…)` before the environment is built
  THEN `lq.evaluate` runs those commands in the configured environment.
- **AC-2** Keywords apply to earlier registrations
  WHEN `lq.configure(conversion="strict", pickle=True, async_loops=2, job_capacity=8)` follows
  registrations
  THEN those commands convert, pickle and run on loops by the new settings.
- **AC-3** The configuration's command registry overrides registered metadata
  WHEN the configuration names a `command_registry:` file describing a registered command
  THEN the built environment reports that file's metadata, and an `import_metadata` call overrides
  both, whether made before or after `configure`.
- **AC-4** Configure after the build is refused
  WHEN `configure` is called after the environment has been built
  THEN it raises `lq.ConfigurationError`, and the environment keeps working with its earlier
  settings, later registrations included.
- **AC-5** Repeated configure
  WHEN `configure` is called twice before the build
  THEN the last call's settings replace every earlier one.
- **AC-6** Invalid configuration leaves the settings unchanged
  WHEN `configure` gets an unparsable configuration, a missing or invalid `command_registry:` file,
  or an invalid keyword value
  THEN it raises at the call, and the earlier settings and every registration remain.
- **AC-7** Every environment
  WHEN `env.configure(...)` is called on a program's own `lq.Environment`
  THEN the same rules apply to it alone, and `lq.configure(...)` is
  `lq.default_environment().configure(...)`.
- **AC-8** Serving and the UI use the configured settings
  WHEN `lq.serve()` or `lq.ui()` runs after `lq.configure(config=…)`
  THEN `lq.serve()` uses the configured `server:` section and store, its keywords still
  overriding them, and `lq.ui()` uses the configured store and settings.
- **AC-9** A failed build can be repaired
  WHEN the first build fails (for example, the store cannot be opened)
  THEN `configure` is still allowed, and the next build uses its settings.
Non-goals: an environment variable naming the configuration file (deferred by Orest, 2026-11-14);
reconfiguring a built environment; replacing the default with another environment. Starting an
application from a configuration file with command-line overrides is the follow-on design
[`py-command-line-config`](../py-command-line-config/DESIGN.md), built on `configure`.

## Core Interactions

- Bindings: `EnvCell` settings become replaceable until the first successful build. Python
  commands and their `Context` read conversion, pickle and the loop pool when they run.
- Server and UI: unchanged. They read the configured settings.

## Crate Placement

`liquers-py` only: the Rust extension (`src/environment.rs`, `src/config.rs`, `src/command.rs`,
`src/aio.rs`) and the Python package.

## Documentation Intent

- Reference: extend `specs/reference/PYTHON_INTEGRATION.md` §"Default environment" and its method
  table: settings stay open until the first successful build.
- Guide: extend `specs/guides/PYTHON_GUIDE.md` "The default environment".
- User guide: chapter 8 §8.1 shows `lq.configure` and §8.2 `env.configure`. Chapter 2's note and
  chapter 12 point to them. The YAML-configured UI and server examples Orest asked for are in
  `py-command-line-config`. Phase 5 reviews every chapter `chapters-for` lists for `affects_docs`.
- Other documents: `liquers-py/python/liquers/__init__.pyi` gains `configure` and
  `Environment.configure`.
- API docs: `liquers-py/python/liquers/_env.py` (its one-line docstring is brought up to
  `API_DOCUMENTATION_GUIDE.md` §3) and `liquers-py/src/environment.rs` (its purpose paragraph and
  retained-declarations section). If the submodule split lands first, the `liquers.environment`
  module docstring instead.

## Open Questions

1. *(decided, Orest 2026-11-14)* `configure` after the build is refused (AC-4). Reconfiguring can
   come later.
2. *(decided, Orest 2026-11-14)* A repeated `configure` replaces every setting (AC-5).
3. *(implementation detail, Phase 2)* Should `configure` re-check declared template and pipeline
   commands against the new settings at the call (AC-6), or leave that to the build?

## Design Readiness

Not assessed yet. It will be assessed after Phase 4.

### Findings carried to Phase 2

- Settings are captured outside `EnvCell`'s plain fields. Each `PyCommandSpec` copies
  `conversion`, `pickle` and the loop pool at registration (`Environment::_register_command`), and
  the Python `Context` inherits them. `EnvCell::new` reads the `command_registry:` file into the
  same `imported` list that `import_metadata` appends to, so it needs its own slot. The
  `LazyLoopPool` size is fixed at construction, but the pool starts at first use. The
  declared-command trial build runs under the settings of its moment. Three readers sit outside
  the declarations lock (`_register_command`, `store_set`, `render_top`).
- Anything that needs the built environment builds it (`EnvCell::envref`): evaluation, assets,
  `serve`, `ui`, templates, editors, `store_set`, and every `lq.Store` and `lq.RecipeProviders`
  method. The getters, `validate()`, `store_types()` and registrations do not.
- Prior art: liquers-web reconfigures after registration (`liquers-web/src/environment.rs`
  `configure_store_on`, `rebuild_with`). Its defect `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`
  is what AC-7 avoids.

## Scope Changes

- 2026-11-14 (Orest, at the Phase 1 gate): asked for the command-line start-up (configuration file
  plus `argparse` overrides) and YAML-configured UI and server examples. Both configure the same
  environment, so they overlap strongly. But together the two exceeded Phase 1's size limit, so
  they were split off into the dependent design `py-command-line-config` with its own issue,
  `PY-APPLICATION-CANNOT-START-FROM-A-CONFIG-FILE`. This design keeps AC-1 to AC-9. The source
  issue was re-sized from `M` to `L` for AC-1 to AC-9 (settings captured in five places, the
  command spec, the loop pool and the configuration slot), and the design converted to the full
  form.

## References

- Source issue: `specs/issues/PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED.md`.
- `py-package-submodules` (`PY-API-IS-ONE-FLAT-MODULE`, Phase 1, unmerged): its rule that every
  public name belongs to one submodule places the new names in `liquers.environment`. Whichever
  design lands second adds them to the other's layout, and neither blocks the other.
- `configuration-document` (the M7 configuration file), `specs/reference/ENVIRONMENT_CONFIG.md`.
