---
id: PY-DEFAULT-ENVIRONMENT-CONFIG
kind: design
title: Configure the Python default environment after commands are registered
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [py, docs]
issues: [PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED]
affects_docs: [specs/guides/PYTHON_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md]
created: 2026-11-14
---
# Configure the Python default environment after commands are registered

## Phase 1: High-Level Design

### Purpose

Let a program set the settings of an environment, the shared default included, after commands
have been registered, so that `lq.configure(config=…)` works whatever order the modules were
imported in. Registrations keep forming the environment's default command set. The configuration,
including its `command_registry:` file, is applied over them at the first successful build, and
registrations still rebuild the environment after it, as they do today.

Today `lq.default_environment()` is a bare `Environment()` whose settings are fixed at
construction, and the first `@lq.command` creates it, usually at import. Requiring `configure`
before any registration was rejected (Orest, 2026-11-14), because a misplaced import would leave
commands in the wrong environment or unregistered.

### Problem Example

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

lq.configure(config=pathlib.Path("liquers.yaml"), pickle=True)   # today: no such function
lq.evaluate("hello")                    # should run with liquers.yaml's store, recipes and settings
lq.serve()                              # should use liquers.yaml's server: section
```

Today there is no `lq.configure`. `lq.Environment(config=…)` makes a second environment that
lacks `hello`, and `lq.evaluate` still runs in the unconfigured default. Expected: one environment
with `hello`, configured by `liquers.yaml`.

### Scope and Acceptance Criteria

"Built" below means that a build of the environment has succeeded once. A later registration
marks the environment for a rebuild (`EnvCell` `retire`) without making it unbuilt again, and a
failed build does not count.

- **AC-1** Configure after import-time registration
  WHEN a module registers commands with `@lq.command` and the program then calls
  `lq.configure(config=…)` before the environment is built
  THEN `lq.evaluate` runs those commands in the configured environment (its store, recipes and
  `python:` / `assets:` settings).
- **AC-2** Keywords apply to earlier registrations
  WHEN `lq.configure(conversion="strict", pickle=True, async_loops=2, job_capacity=8)` is called
  after commands were registered
  THEN those commands convert, pickle and run on loops by the new settings, not those in force
  when they were registered.
- **AC-3** The configuration's command registry overrides registered metadata
  WHEN the configuration names a `command_registry:` file describing a registered command
  THEN that file's metadata is what the built environment reports for the command, and an
  `import_metadata` call overrides both, whether made before or after `configure`.
- **AC-4** Configure after the environment is built is refused
  WHEN `configure` is called after the environment has been built
  THEN it raises `lq.ConfigurationError`, saying the environment is already built, and the
  environment keeps working with its earlier settings, later registrations included.
- **AC-5** Repeated configure before the build
  WHEN `configure` is called twice before the environment is built
  THEN the last call's settings are the ones used.
- **AC-6** Invalid configuration leaves the settings unchanged
  WHEN `configure` is given a configuration that fails to parse, a `command_registry:` file that
  is missing or invalid, or an invalid keyword value
  THEN it raises at the call, and the earlier settings and every registration remain.
- **AC-7** Every environment, not only the default
  WHEN `env.configure(...)` is called on an `lq.Environment` made by the program
  THEN the same rules apply to that environment alone, and `lq.configure(...)` is
  `lq.default_environment().configure(...)`.
- **AC-8** Serving and the UI use the configured settings
  WHEN `lq.serve()` or `lq.ui()` runs after `lq.configure(config=…)`
  THEN `lq.serve()` uses the configuration's `server:` section and store, with its keywords still
  overriding them, and `lq.ui()` uses the configured store and settings.
- **AC-9** A failed build can be repaired
  WHEN the first build fails (for example, the configured store cannot be opened)
  THEN `configure` is still allowed, and the next build uses its settings.

Non-goals: an environment variable naming a configuration file (deferred by Orest, 2026-11-14);
reconfiguring an environment that is already built; replacing the default with another
environment (`set_default_environment`).

### Constraints Found for Phase 2

The settings are captured in more places than the environment's own fields, and each needs a
late-bound form: `EnvCell` `config`, `conversion`, `pickle` (plain fields of the shared cell);
each `PyCommandSpec`'s callable, which copies `conversion`, `pickle` and the loop pool at
registration (`Environment::_register_command`), and the Python `Context` inherits from it; the
`command_registry:` file, read in `EnvCell::new` into the same `imported` list as
`import_metadata`; the `LazyLoopPool` size, set at construction but started only at first use;
the declared-command trial build (`template_command`, `pipeline_command`), which runs
`configured_builder` under the settings of that moment; and three readers outside the declarations
lock (`_register_command`, `store_set`'s conversion, `render_top`), which a `configure` on another
thread could race. Any call that needs the built environment builds it (`EnvCell::envref`):
evaluation, assets, `serve`, `ui`, templates, editors, `store_set`, and every `lq.Store` and
`lq.RecipeProviders` method. The getters, `validate()`, `store_types()` and registrations do not.

Prior art: liquers-web already reconfigures after registration (`liquers-web/src/environment.rs`
`configure_store_on`, `rebuild_with`). Its defect `WEB-EXPLICIT-ENVIRONMENT-CONFIGURES-THE-SINGLETON`
is what AC-7 avoids by making `configure` act on its own environment.

### Documentation Intent

- User guide: `docs/guides/python/08-environment.md` §8.1 shows `lq.configure`, and §8.2 adds
  `env.configure`. Chapter 2's "The default environment" note and chapter 12 (serving the default
  environment) point to it.
- `specs/guides/PYTHON_GUIDE.md` "The default environment", and
  `specs/reference/PYTHON_INTEGRATION.md` §"Default environment" and the method table: the rule
  that settings may change until the first build.
- Type stubs: `liquers-py/python/liquers/__init__.pyi` gains `configure` and
  `Environment.configure`.
- Phase 5 reviews every chapter `python3 scripts/docs_index.py chapters-for` lists for
  `affects_docs`. Only chapters 02, 08 and 12 are expected to change.
- API docs: `liquers-py/python/liquers/_env.py` (its one-line module docstring is brought up to
  `API_DOCUMENTATION_GUIDE.md` §3) and `liquers-py/src/environment.rs` (module doc: its purpose
  paragraph and the section on retained declarations, which say that settings stay open until the
  first successful build). If the submodule split lands first, the `liquers.environment`
  module docstring instead.

### Design Dependencies

- `py-package-submodules` (PY-API-IS-ONE-FLAT-MODULE, Phase 1, unmerged) puts
  `default_environment` in `liquers.environment`. `configure` goes beside it, with a top-level
  name either way. Neither design blocks the other, and the one that lands second adds the name to
  the other's layout.

### Open Questions

1. *(proposed resolution)* **`configure` after the environment is built.** Recommended: refuse
   (AC-4). A rebuild keeps the store, but a new configuration could name another store under
   running servers and UIs, the loop pool has started, and stored results were made under the old
   settings. Reconfiguring can be added later.
2. *(open design)* **What a repeated `configure` keeps.** Recommended: each call replaces every
   setting, as a new `Environment(...)` would, so `configure(pickle=True)` after
   `configure(config=p)` drops `p`. The alternative merges keyword by keyword, which is harder to
   explain when a `config` and a keyword both set `pickle`.
3. *(implementation detail, Phase 2)* Whether `configure` re-checks declared template and pipeline
   commands against the new settings at the call (fits AC-6), or leaves that to the build.
