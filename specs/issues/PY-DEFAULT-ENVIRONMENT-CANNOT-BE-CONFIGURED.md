---
id: PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED
kind: feature
title: The Python default environment cannot be configured, and the guide does not say so
status: draft
priority: P2
complexity: M
area: [py, docs]
design:
created: 2026-11-14
github:
---

## Problem

`lq.default_environment()` always makes a bare `Environment()`
(`liquers-py/python/liquers/_env.py`, `default_environment`). Nothing passes it settings: no
argument, no setter, no environment variable and no configuration file it looks for.
`PyConfig::from_python(None)` returns `PyConfig::default()` (`liquers-py/src/config.rs`), so it
gets a memory store, the default recipe chain, `conversion="permissive"`, `pickle=False`,
`async_loops=1` and the default `job_capacity`.

Every setting of an environment is fixed when it is made. `Environment.__new__` takes `config`,
`conversion`, `pickle`, `async_loops` and `job_capacity` (`liquers-py/src/environment.rs`,
`#[new]`), and `EnvCell::new` stores them. The store, the recipe providers and the `server:` /
`ui` settings come only from `config`. Once the default exists, none of this can change.

What reaches the default environment after it is made is limited to registration:
`@lq.command`, `@lq.first_command`, `register_store_factory`, `register_recipe_provider_factory`
and `import_metadata`. Settings are not reachable this way. `lq.serve(**settings)` overrides the
`server:` section for one server, but the default environment has no `server:` section to start
from.

The only way to change it today is to assign the private `liquers._env._DEFAULT`, which nothing
documents or supports.

The bare decorators make it harder. `@lq.command` calls `default_environment()` when the
decorator runs (`liquers-py/python/liquers/_commands.py`, `_decorator`), which is usually at
import time. So the default exists as soon as any module that registers a command is imported,
before a program's `main` can set anything.

**The documentation.** The user guide says the default environment is "created the first time it
is needed, with default settings" (`docs/guides/python/08-environment.md` §8.1, also chapter 2
"The default environment") and sends readers to their own `lq.Environment` for a store on disk or
other options. It never says that the default cannot be configured, or that you then lose
`lq.evaluate`, `lq.serve`, `lq.ui` and the bare decorators and must use `env.evaluate`,
`env.serve`, `env.ui` and `@env.command`. `specs/guides/PYTHON_GUIDE.md` and
`specs/reference/PYTHON_INTEGRATION.md` §"Default environment" say the same and stop there.

## Problem Example

```python
import liquers as lq

# Wanted: lq.evaluate, @lq.command and lq.serve on a store on disk, with pickle on.
# There is no call that does this. Making your own environment works,
env = lq.Environment(config=pathlib.Path("liquers.yaml"), pickle=True)
# but lq.evaluate("hello") still runs in a second, unconfigured environment,
# and commands registered with @lq.command in other modules are not in `env`.
```

## Impact

A script that grows a configuration file has to rewrite every `lq.evaluate`, `lq.serve`,
`lq.ui` and `@lq.command` into the `env.` form, including in library modules that register
commands. The shared default is meant for that kind of script (one environment per process). A
library cannot register its commands in "whatever environment the application configured"
without making the application pass an environment around.

Workaround: create an `lq.Environment` and use its methods and decorators everywhere. It works,
but the guide does not say it is needed.

## Expected behaviour

The default environment can take the same settings as `lq.Environment`, at least a configuration
document (which already carries the store, recipes, `server:`, `python:` and `assets:` sections
and `command_registry:`), in a documented way that does not depend on import order. The user
guide (chapter 8, chapter 2's note, chapter 12) and `PYTHON_GUIDE.md` show it.

Ways to get there, which can be combined:

1. **Configure before first use**: `lq.configure(config=…, pickle=…, …)`, taking
   `Environment`'s keywords. It fails if the default already exists, which import-time decorators
   make likely unless (3) is done too.
2. **Replace the default**: `lq.set_default_environment(env)`, where `env` is an
   `lq.Environment` the program made. It has to decide what happens to commands already
   registered in the old default: move them, refuse, or drop them.
3. **Make the default's settings late-bound**: the bare decorators record declarations without
   fixing the settings, and the environment takes its settings at its first build (first
   evaluate, serve or ui). This makes (1) work after imports. The declarations are already
   applied lazily at build (`EnvCell::configured_builder`). Only `config`, `conversion`,
   `pickle` and `async_loops` are fixed in `EnvCell::new`.
4. **An environment variable** such as `LIQUERS_CONFIG=<path>`, read when the default is made.
   It needs no code and suits deployment, but cannot set what the file does not.

The submodule split (`PY-API-IS-ONE-FLAT-MODULE`, design `py-package-submodules`) puts
`default_environment` in `liquers.environment`, so any new function goes there too, with a
top-level name.

## Discovery

Orest asked on 2026-11-14 whether the default environment can be configured, since the user
documentation does not show how. Tracing `_env.py`, `Environment.__new__`, `EnvCell::new`,
`PyConfig::from_python` and `_commands.py` at `2c1a94e` showed no way to do it.
