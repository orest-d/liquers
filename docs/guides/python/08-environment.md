---
title: The environment
status: ready
sources: [reference/ENVIRONMENT_CONFIG.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-15
---
# 8. The environment

PURPOSE: The reader can build and configure an environment with the commands, stores and recipes an application needs, start an application from a configuration file and its command line, and use the environment from threads and from asyncio.

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

Left alone, it has the default settings: a store in memory and the commands of the library.
That suits a script and a notebook. To give it a store on disk, a server section or any other
setting, call `lq.configure` with the same keywords as `lq.Environment` (section 8.2):

```python
import pathlib, tempfile
import liquers as lq

@lq.first_command                       # often in a module you import: that is fine
def hello() -> str:
    return "hello"

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "liquers.yaml").write_text(
    "store:\n"
    "  stores:\n"
    "    - {type: filesystem, prefix: data, config: {path: " + str(folder / "data") + "} }\n"
)
lq.configure(config=folder / "liquers.yaml")
assert lq.evaluate("hello") == "hello"
lq.default_environment().store_set("data/note.txt", "kept")
assert (folder / "data" / "data" / "note.txt").read_text() == "kept"
```

`lq.configure` can come after the commands are registered, in whatever order your modules are
imported. It must come before the environment's first use: the first evaluation, `lq.serve`,
`lq.ui` or a store write. After that it raises `ConfigurationError` ("already built"), and the
environment carries on with the settings it has. Calling it twice before that is allowed: the
second call replaces every setting of the first, so give all of them at once. A configuration
that is wrong (a section Liquers does not know, a missing command metadata file) raises at the
call and changes nothing.

Create your own `lq.Environment` when you need any of these:

- isolation: tests that must not see each other's commands, or two configurations in one
  process;
- an environment you shut down and replace while the program runs.

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

`env.configure(...)` takes the same keywords and sets them later, until the environment's first
use, as `lq.configure` does for the default environment. Both also take `overrides`, a partial
configuration merged over `config` (section 8.7):

```python
import liquers as lq
import pytest

env = lq.Environment()

@env.first_command
def two() -> int:
    return 2

env.configure(conversion="strict", job_capacity=8)
assert env.evaluate("two") == 2             # the first use: the settings are fixed now
with pytest.raises(lq.ConfigurationError, match="already built"):
    env.configure(pickle=True)
env.shutdown()
```

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
`"Greet someone"`. The file is read when the environment is created or configured, so a missing
or malformed file raises at once, naming it. It describes commands registered before or after
that, and `env.import_metadata(...)` overrides what it says. An entry naming a command nobody registered is reported by
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
`conversion` or a malformed `config` is refused by the constructor or by `configure`. A store of an unknown type
or an unknown source of recipes is reported by the first evaluation.

`env.validate()` checks the registered commands without building the environment, and returns a
list of `lq.Issue` objects, empty when all is well. It does not check the store configuration.

Registering a command, a store factory or a recipe provider after the first use rebuilds the
environment, with the settings it already has. The store is kept; the results computed so far are
not. Set everything up first, then evaluate. If the first build fails, for example on a store
that cannot be opened, the environment counts as not yet used: `configure` it again and
evaluate.

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

## 8.7 Starting an application

An application usually starts from a configuration file and lets its command line override a
setting or two, such as the port. `lq.add_arguments(parser)` adds the Liquers options to your
own `argparse` parser, in a group of their own:

| Option | Sets |
|---|---|
| `--config FILE` | the configuration file; without it, `liquers.yaml` in the working directory if there is one |
| `--command-registry FILE` | `command_registry` |
| `--default-namespace NS` | `default_namespaces` (repeat it for more) |
| `--host`, `--port`, `--prefix` | `server.host`, `server.port`, `server.prefix` |
| `--server-read-only`, `--no-server-read-only` | `server.read_only` |
| `--job-capacity N`, `--async-loops N` | `assets.job_capacity`, `python.async_loops` |
| `--conversion`, `--pickle`, `--no-pickle` | `python.conversion`, `python.pickle` |

An option you do not give leaves the file's value. `lq.configure_from_args(args)` then
configures the default environment from the file with the given options merged over it, step by
step:

```python
import argparse, pathlib, tempfile
import liquers as lq

@lq.first_command
def hello() -> str:
    return "hello"

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "app.yaml").write_text("server: {host: 127.0.0.1, port: 8123, prefix: /app}\n")

parser = argparse.ArgumentParser(description="My Liquers app")
parser.add_argument("--title", default="My app")          # your program's own option
lq.add_arguments(parser)
args = parser.parse_args(["--config", str(folder / "app.yaml"), "--port", "0"])
lq.configure_from_args(args)

with lq.serve() as server:                                 # the file's prefix, port 0: a free port
    assert server.port != 8123 and args.title == "My app"
```

A real program calls `parser.parse_args()` with no list, which reads its command line:
`python app.py --config app.yaml --port 9000`. `lq.configure_from_command_line` does the three
steps in one call and returns the parsed arguments:

```python
import argparse, pathlib, tempfile
import liquers as lq

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "app.yaml").write_text("server: {port: 8123}\npython: {conversion: strict}\n")

parser = argparse.ArgumentParser(description="My Liquers app")
parser.add_argument("--title", default="My app")
args = lq.configure_from_command_line(parser, argv=["--config", str(folder / "app.yaml"), "--port", "0"])
assert args.title == "My app" and args.liquers_port == 0
```

Both take `env=` to configure an environment of your own instead of the default. Leave out a
Liquers option your program already uses with `options=["config", "port"]`, which adds only
those; without it, an option name your parser already has is an `argparse.ArgumentError`.
`default_config=None` turns off the default `liquers.yaml`. A `--config` that does not exist is a
`ConfigurationError`.

Paths keep their meaning. A relative `command_registry` in the file is found next to the file,
and a relative `--command-registry` next to where the program was started, because the options
are merged into the parsed file instead of into its text. To merge settings of your own the same
way, pass them to `configure` as `overrides`, a partial configuration in the shape of the file:

```python
import pathlib, tempfile
import liquers as lq

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "app.yaml").write_text("server: {host: 127.0.0.1, port: 8123}\n")
lq.configure(config=folder / "app.yaml", overrides={"server": {"port": 0}})
with lq.serve() as server:                                 # host from the file, port 0 merged in
    assert server.port != 8123
```

`--pickle` and the server options change what the program trusts and exposes; chapter 12 says
when each is safe.
