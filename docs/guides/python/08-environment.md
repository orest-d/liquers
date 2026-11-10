---
title: The environment
status: ready
sources: [reference/ENVIRONMENT_CONFIG.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-10
---
# 8. The environment

PURPOSE: The reader can build an environment with the commands, stores and recipes an application needs, and use it from threads and from asyncio.

An *environment* is everything a query needs in order to run: the commands it may call, the
store it reads and writes, the recipes, and the machinery that evaluates and caches. The earlier
chapters each configured one part of it. This chapter puts the parts together and covers how an
environment lives and how it behaves under concurrency.

## 8.1 The default environment and your own

The package has one environment ready for you. `lq.evaluate`, `@lq.command` and
`@lq.first_command` use it, and `lq.default_environment()` returns it:

```python
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

assert lq.evaluate("hello") == "hello"
assert lq.default_environment().evaluate("hello") == "hello"
```

It is created the first time it is needed, with default settings: a store in memory and the
commands of the library. That suits a script and a notebook.

Create your own `lq.Environment` when you need any of these:

- a store on disk or elsewhere (chapter 6);
- isolation: tests that must not see each other's commands, or two configurations in one
  process;
- control over the options below.

An environment you create has its own decorators, `@env.command` and `@env.first_command`, and
its own `evaluate`. Nothing is shared between two environments.

## 8.2 Creating an environment

`lq.Environment` takes keyword arguments only:

| Argument | Default | Meaning |
|---|---|---|
| `config` | none | the configuration: a file as a `pathlib.Path`, a `dict`, or YAML text (section 8.3) |
| `conversion` | `"permissive"` | what happens to a Python object Liquers has no type for: `"permissive"` keeps it as an opaque object, `"strict"` refuses it (chapter 4) |
| `pickle` | `False` | whether opaque objects may be written to the store with `pickle`; only for stores you trust |
| `job_capacity` | 4 | how many commands run at the same time (section 8.6) |
| `async_loops` | 1 | how many event loops serve async commands (section 8.6) |

```python
import liquers as lq

env = lq.Environment(conversion="strict", job_capacity=8)

@env.first_command
def hello() -> str:
    return "hello"

assert env.evaluate("hello") == "hello"
env.shutdown()
```

The keywords `conversion`, `pickle`, `job_capacity` and `async_loops` can also be set in the
configuration. A keyword you give wins over the configuration, and the configuration wins over
the default, so one configuration file can serve a laptop and a server that differ in a setting
or two:

```python
import liquers as lq

config = {"assets": {"job_capacity": 8}, "python": {"pickle": False}}
env = lq.Environment(config=config, job_capacity=2)   # 2 commands at a time, not 8
env.shutdown()
```

In the same way, a keyword of `env.serve()` overrides the configuration's `server` section
(chapter 12).

## 8.3 The configuration

One configuration describes the whole environment: where data lives, where recipes come from,
how results are kept, the command metadata, the web server and this package's own settings. Each
section can be left out:

```yaml
store:                      # where data lives (chapter 6)
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
    - type: memory
      prefix: scratch
recipes:                    # where recipes come from (chapter 7)
  - recipes_yaml
  - manifest
assets:                     # how results are evaluated and kept (chapter 7)
  job_capacity: 8
  dependency_audit: on_load
default_namespaces: [pl]    # namespaces searched without an `ns-` in the query
command_registry: commands.yaml   # descriptions of your commands (below)
python:                     # this package's settings
  pickle: false
server:                     # the web server (chapter 12)
  port: 8000
```

| Section | Left out, it means |
|---|---|
| `store` | one store in memory |
| `recipes` | `recipes.yaml` files and table manifests |
| `assets` | the defaults of chapter 7 |
| `default_namespaces` | only commands without a namespace are found without `ns-` |
| `command_registry` | no command metadata file |
| `python` | `conversion`, `pickle` and `async_loops` at the defaults of section 8.2; leave `pickle` off unless you trust everything the store holds, because reading pickle runs code |
| `server` | the server defaults of chapter 12 |

Keep the configuration in a file and pass it as a `pathlib.Path`: YAML (`.yaml`, `.yml`) or JSON
(`.json`). A `str` is always YAML text, never a file name, so
`lq.Environment(config="liquers.yaml")` raises and tells you to use `pathlib.Path`. An empty file,
`{}` and no `config` at all are the same: every default.

```python
import pathlib, tempfile
import liquers as lq

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "liquers.yaml").write_text("assets:\n  job_capacity: 2\nserver:\n  port: 0\n")
env = lq.Environment(config=folder / "liquers.yaml")
env.shutdown()
```

A section Liquers does not know is an error that names it and lists the known ones, so a typo
such as `sever:` never goes unnoticed. The same holds for a misspelled key inside `python` and
`server`. A relative file name in the configuration, such as `command_registry`, is found next to
the file; store paths are relative to the working directory. A YAML value can name an
environment variable, as the store path above does with `LIQUERS_DATA`; the variable is read when
the store is built, and an unset one is an error.

`default_namespaces` saves typing when most of your queries use one library. With `[pl]`, the
query `-R/data/sales.csv/-/from_csv/head-2` finds `from_csv` and `head` in the namespace `pl`.
A query that names its namespaces works in every environment, so prefer the explicit form in
recipes and in anything you share.

The configuration contains no commands. Commands are code, and are registered with the
decorators.

### Command metadata from a file

A *command metadata file* describes commands your code registers: their labels, documentation,
argument names and hints for user interfaces. It never registers a command; it only describes
the ones you register. Name it in the configuration with `command_registry`:

```yaml
# commands.yaml
commands:
  - name: greet
    label: Greet someone
    doc: Returns a greeting for a name.
    module: python
    state_argument: null
    arguments:
      - {name: name, label: Name, default: !Value world, argument_type: string, gui_info: !TextField 20}
```

After `@env.first_command def greet(...)`, `env.command_metadata("greet").label` is
`"Greet someone"`. The file is read when the environment is created, so a missing or malformed
file raises at once, naming it. An entry naming a command nobody registered is reported by
`env.validate()` and fails the next build.

## 8.4 The life of an environment

An environment is built the first time it is used, from the configuration and from what was
registered until then. From that moment it runs until you shut it down.

```python
import liquers as lq

env = lq.Environment()
try:
    assert env.evaluate("commands_doc").startswith("# Commands")
finally:
    env.shutdown()

try:
    env.evaluate("commands_doc")
except lq.GeneralError:
    pass                                # an environment cannot be used after shutdown
```

`shutdown()` stops the evaluations that are still running, ends any wait that is still
outstanding with `CancelledError`, and releases the stores. Calling it twice is harmless. Call
it when you are done with an environment you created, typically in a `finally` block or in a
test fixture (chapter 14). The default environment is shut down when the program exits.

Because the environment is built at first use, some mistakes show up late. An invalid
`conversion` or a malformed `config` is refused by the constructor. A store of an unknown type
or an unknown source of recipes is reported by the first evaluation.

`env.validate()` checks the registered commands without building the environment, and returns a
list of `lq.Issue` objects, empty when all is well. It does not check the store configuration.

Registering a command, a store factory or a recipe provider after the first use rebuilds the
environment. The store is kept; the results computed so far are not. Set everything up first,
then evaluate.

## 8.5 Using an environment from asyncio

Every call that waits has a form to `await`. Use it in async code, so that waiting for a result
does not block your event loop:

```python
import asyncio
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

async def main():
    assert await env.evaluate_async("hello") == "hello"
    both = await asyncio.gather(env.evaluate_async("hello"), env.evaluate_async("hello"))
    assert both == ["hello", "hello"]
    asset = await env.get_asset_async("hello")
    assert await asset.get_async() == "hello"

asyncio.run(main())
env.shutdown()
```

| Blocking | Awaitable |
|---|---|
| `env.evaluate`, `env.evaluate_state`, `env.get_asset`, `env.store_set` | `env.evaluate_async`, `env.evaluate_state_async`, `env.get_asset_async`, `env.store_set_async` |
| `asset.get`, `asset.get_state`, `asset.cancel` | `asset.get_async`, `asset.get_state_async`, `asset.cancel_async` |
| `context.evaluate` | `context.evaluate_async` |
| `lq.evaluate` | `lq.evaluate_async` |

Three rules:

- Cancelling the task that awaits ends only *your wait*. The evaluation finishes and its result
  is kept. To stop the computation itself, `await asset.cancel_async()`.
- For a time limit, wrap the await in `asyncio.timeout(…)`.
- A blocking call inside a running event loop still works, but it holds up the loop and warns
  with `RuntimeWarning`.

## 8.6 Threads and capacity

One environment can be used from many Python threads at once. Evaluations run in parallel, and
while a thread waits for a result, it does not hold the interpreter lock, so other threads keep
running.

`job_capacity` limits how many commands run at the same time. The others wait in a queue, with
the status `Submitted`. All commands share this one limit. A command that calls
`context.evaluate` does not wait for a slot that it holds itself, so nested evaluation is safe.
A command that calls `env.evaluate` instead can wait for ever when the capacity is used up,
which is one more reason to use the context inside commands.

Async commands run on event loops that belong to the environment. There is one unless you ask
for more with `async_loops`; each loop has its own thread.

On a free-threaded build of Python (`python3.14t`), `liquers` keeps the interpreter lock off, so
commands called from different threads really run at the same time. Two things follow:

- A command can run on several threads at once. Protect the state it shares, for example with a
  `threading.Lock` around a counter.
- Do not change one `lq.Query`, `lq.Key` or `lq.Metadata` object from two threads at once. The
  environment, assets, states and contexts are safe to share.
