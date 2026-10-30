---
title: Using Liquers from Python
kind: guide
audience: user
area: [py]
reviewed: 2026-10-27
---
# Using Liquers from Python

How to use Liquers from Python with the `liquers` package: build it, configure an environment,
register commands written in Python, and evaluate queries. How the package is built on the Rust
crates, and the exact rules it follows, are in
[`reference/PYTHON_INTEGRATION.md`](../reference/PYTHON_INTEGRATION.md).

Every example below runs as written against the package built from this repository;
`liquers-py/tests/test_GUIDE_examples.py` runs them all. Examples that use `polars` need it
installed.

## 1. Installing

The package is not published yet. Build it from the repository root into a virtual environment
with [maturin](https://www.maturin.rs):

```bash
python -m venv .venv && . .venv/bin/activate
pip install maturin polars pillow
maturin develop --release -m liquers-py/Cargo.toml   # builds the extension and installs `liquers`
```

The first build compiles the Rust libraries, Polars included, so it takes several minutes and a few
gigabytes of disk. `--release` makes the package much faster; leave it out for a quicker build
while working on the bindings themselves. A Rust toolchain is required. `polars` and `pillow` are
optional: they are needed only to exchange DataFrames and images.

The distribution is called `liquers-py`; the package is imported as `liquers`, conventionally as
`lq`. CPython 3.9 to 3.15 and PyPy 3.11 or newer are supported.

Liquers writes progress lines to stderr while it evaluates, and they cannot be turned off yet
(`CORE-TRACING-MIGRATION`). Stdout is left alone.

## 2. What is in the package

Everything is imported from `liquers`; the underscored modules are implementation, listed so a
traceback is readable.

| Module | What it holds |
|---|---|
| `liquers` | The public names, all re-exported here. |
| `liquers._native` | The Rust extension: every class below except the exception classes, and `parse`, `parse_key`, `build_plan`. |
| `liquers._env` | `default_environment()` and `evaluate()`. |
| `liquers._commands` | The `command` and `first_command` decorators. |
| `liquers._errors` | `LiquersError` and its subclasses, one per `ErrorType`. |

The public names, by purpose:

| Purpose | Names |
|---|---|
| Running queries | `Environment`, `default_environment`, `evaluate`, `AssetRef`, `State` |
| Writing commands | `command`, `first_command`, `Context` |
| Errors | `LiquersError`, `KeyNotFoundError`, `ExecutionError`, … (one per `ErrorType`), `ErrorType` |
| Results' metadata | `Metadata`, `MetadataRecord`, `AssetInfo`, `Status`, `LogEntry`, `LogEntryKind`, `Version` |
| The query language | `parse`, `parse_key`, `Query`, `Key`, `ActionRequest`, `ActionParameter`, the segment classes, `Position` |
| Commands' metadata | `CommandMetadata`, `CommandKey`, `ArgumentInfo`, `ArgumentType`, `CommandMetadataRegistry`, … |
| Plans and recipes | `build_plan`, `Plan`, `Step`, `Recipe`, `RecipeList` |
| Configuration checks | `Issue` (from `Environment.validate()`) |

`lq.Error` is a plain data object describing a Liquers error. It is not an exception; what is raised
is a `LiquersError`.

## 3. A first query

```python
import liquers as lq

listing = lq.evaluate("commands_doc")       # Markdown: every command you can use
assert listing.startswith("# Commands")
```

`lq.evaluate` runs a query in the process's default environment and returns its value as a Python
object. Every environment comes with the built-in commands of `liquers-lib`: `to_text`,
`commands_doc`, the Polars commands under `ns-pl`, images, records, search and templates.
`commands_doc` lists them all, with their arguments.

A query is a path of actions: `hello/repeat-3` applies `hello`, then `repeat` with the argument
`3`. `-R/data/note.txt` reads a key from the store, and `-R/data/note.txt/-/to_text` reads it and
applies `to_text` (the `/-/` starts the actions; without it, `to_text` would be part of the key).
The query language is described in
[`reference/PROJECT_OVERVIEW.md`](../reference/PROJECT_OVERVIEW.md).

Each result Liquers computes is an *asset*: the value together with its status (such as
`Processing` or `Ready`) and its metadata (type, log, and how it was made).

## 4. Configuring an environment

The default environment suits a script. Anything else — tests, a store on disk, several
configurations in one process — makes its own `lq.Environment`:

```python
import tempfile
import liquers as lq

root = tempfile.mkdtemp()
env = lq.Environment(config={
    "store": {"stores": [{"type": "filesystem", "prefix": "data", "config": {"path": root}}]},
    "assets": {"job_capacity": 2},
})
env.store_set("data/note.txt", "kept")      # the file <root>/data/note.txt
assert env.evaluate("-R/data/note.txt") == "kept"
env.shutdown()
```

The prefix stays in the path: under this configuration the key `data/note.txt` is the file
`<root>/data/note.txt`, so point `path` at the folder that *contains* `data/`.

`Environment` takes keyword arguments only:

| Argument | Default | Meaning |
|---|---|---|
| `config` | none | The environment configuration, as a `dict` or a YAML string: `store` (where data lives), `assets` (asset manager options). The format is [`reference/ENVIRONMENT_CONFIG.md`](../reference/ENVIRONMENT_CONFIG.md). With no `store`, the store is in memory. `recipes` must be left at `default`. |
| `conversion` | `"permissive"` | What happens to a Python object with no Liquers equivalent: `"permissive"` keeps it as an opaque object, `"strict"` refuses it with `ConversionError` (§7). |
| `pickle` | `False` | Whether opaque objects can be written to the store, as pickle. Leave it off unless you trust everything the store holds: reading pickle runs code. |

A configuration is usually kept in a file. YAML may refer to environment variables as
`${NAME}`:

```python
import os
import tempfile
import liquers as lq

os.environ["LIQUERS_DATA"] = tempfile.mkdtemp()
config_text = """
store:
  stores:
    - type: filesystem
      prefix: data
      config:
        path: ${LIQUERS_DATA}
assets:
  job_capacity: 2
"""
# In practice: config_text = pathlib.Path("liquers.yaml").read_text()
env = lq.Environment(config=config_text)
env.store_set("data/note.txt", "kept")
env.shutdown()
```

`config` must be the text, not a path. Malformed YAML raises `ParseError`; keys Liquers does not
know are ignored, so check the spelling of a setting that seems to have no effect.

**Lifecycle.** An environment is built on first use, from the commands registered on it, and keeps
running until `shutdown()`. `shutdown()` is idempotent; afterwards every use raises `GeneralError`
(a wait still outstanding raises `CancelledError`). Call it when you are done with an environment
you made, typically in a `finally` or a test fixture. An invalid `conversion` or `recipes` is
refused by the constructor, but a store that cannot be built, such as one of an unknown type
(`NotSupportedError`), is reported by the first use.

**The default environment.** `lq.default_environment()` returns one shared environment, created on
first use with the default settings. `lq.evaluate` and the bare `@lq.command` decorators use it.

**Threads.** One environment can be used from several Python threads at once; evaluations run in
parallel.

## 5. Registering a command

A command is a Python function registered with a decorator. `@lq.first_command` registers a
function that starts a query; `@lq.command` registers one that transforms the value before it:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command
def repeat(text: str, count: int = 2) -> str:
    """Repeat the input text."""
    return text * count

assert env.evaluate("hello/repeat-3") == "hellohellohello"
assert env.evaluate("hello/repeat") == "hellohello"         # the default
env.shutdown()
```

`@env.command` registers in `env`; `@lq.command` registers in the default environment, and
`@lq.command(env=env)` is the same as `@env.command`. The function is returned unchanged, so it
can still be called directly. `ns`, `q` and `v` are query instructions, not command names: a
command with any of these names is refused when it is registered.

### The input

The first parameter of a `command` receives the input, and its **name** says in what form:

| First parameter | Receives |
|---|---|
| `state` | an `lq.State`: `.value`, and `.metadata` with the type, log and status |
| `text` | the input converted to a `str` |
| any other name | the input value |

`first_command` has no input, so its first parameter is an ordinary argument.

### Arguments

Every other parameter is an argument, given in the query after the command name, separated by
`-`: `repeat-3`. Annotations convert the text of the query:

| Annotation | Argument type |
|---|---|
| `str`, `int`, `float`, `bool` | that type |
| `Optional[int]`, `Optional[float]`, `Optional[bool]` (or `int \| None`, …) | optional; an empty or omitted argument gives the default, or `None` |
| none, or anything else | the query text as a `str`; an omitted argument gets its default unchanged |

Annotate a number, or it arrives as text. For an optional string use `str = ""`; `Optional[str]`
counts as unannotated. A default must be `None`, a `str`, an `int`, a finite `float` or a `bool`.

An argument containing `-`, `/` or a space is written with an escape: `~_` for `-`, `~/` for `/`,
`~.` for a space, `~~` for `~`. The full encoding is in
[`reference/PROJECT_OVERVIEW.md`](../reference/PROJECT_OVERVIEW.md).

`*args` takes every remaining argument:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command
def join(value, *parts: str) -> str:
    return "-".join([value, *parts])

assert env.evaluate("hello/join-a-b-c") == "hello-a-b-c"
assert env.evaluate("hello/join-a~_b") == "hello-a-b"       # one argument, "a-b"
env.shutdown()
```

A keyword-only parameter or `**kwargs` cannot be given in a query, so the decorator refuses it with
`TypeError`.

### The context

A parameter named `context` receives an `lq.Context` and is not an argument. It lets a command log
to its result's metadata, evaluate another query as a dependency, and check for cancellation:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.first_command
def greeting(context, name: str = "world") -> str:
    context.info(f"greeting {name}")
    return f"{context.evaluate('hello')}, {name}"

assert env.evaluate("greeting-Ada") == "hello, Ada"
env.shutdown()
```

| `Context` method | Does |
|---|---|
| `evaluate(query)` | evaluates `query` as a dependency of this result and returns its value |
| `info(msg)`, `warning(msg)` | adds an entry to the result's log |
| `error(msg)` | adds an error entry **and fails the result**: the command keeps running, but once it returns the query raises `ExecutionError` with the first `msg` |
| `is_cancelled()` | whether the result was cancelled; a long command should check it and stop |

The context is valid only during the call. Keeping it and using it later raises `GeneralError`.
Put `context` before `*args`; after it, it would be keyword-only.

### Metadata

Both decorators take keyword arguments describing the command:

| Keyword | Meaning |
|---|---|
| `name` | the command's name in queries; default: the function's name |
| `namespace` | a group of commands; a query selects it with `ns-<namespace>` before the command |
| `label`, `doc` | a display name and a description; `doc` defaults to the docstring |
| `filename` | the default file name a client offers for the result, such as `"result.csv"`; recorded in the command's metadata, it changes neither the value nor the result's metadata |
| `volatile` | `True` for a result that must never be reused |
| `cached` | `False` for a cheap command whose result is not worth keeping |
| `env` | the environment to register in |

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command(namespace="demo", label="Shout")
def shout(text):
    return text.upper()

assert env.evaluate("hello/ns-demo/shout") == "HELLO"
assert env.command_metadata("shout", "demo").label == "Shout"
env.shutdown()
```

### Replacing and removing commands

Registering a name that already exists replaces it, with a warning naming what was replaced — a
built-in command can be overridden this way. `env.unregister(name, namespace="")` removes the
command and returns whether there was one; removing a Python command that replaced a built-in
brings the built-in back.

Register commands before the first evaluation where you can. A command registered later takes
effect on the next evaluation, by rebuilding the environment: the store is kept, but results
already computed are not, and the replaced environment is kept until `shutdown()`
(`LANGUAGE-INTEGRATIONS-REBUILD-ENVIRONMENT-ON-REGISTRATION`).

### Async commands

A command defined with `async def` is awaited for you. Use it for work that waits on something
else, such as a network call, so the waiting does not hold a thread:

```python
import asyncio
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command
async def fetch_len(text: str) -> int:
    await asyncio.sleep(0.01)          # stands in for an HTTP call
    return len(text)

@env.first_command
async def outer(context) -> str:
    # Inside an async command, evaluate with `await context.evaluate_async(...)`.
    return f"length {await context.evaluate_async('hello/fetch_len')}"

assert env.evaluate("hello/fetch_len") == 5
assert env.evaluate("outer") == "length 5"
assert env.command_metadata("fetch_len").is_async
env.shutdown()
```

The coroutine runs on an event loop that belongs to the environment, never on yours, so it may be
evaluated from plain code (as above) or awaited (below). An environment has one such loop unless
you ask for more with `lq.Environment(async_loops=4)`; each loop has its own thread, and a command's
calls are spread over them.

Inside an `async def` command, the blocking calls (`context.evaluate`, `env.evaluate`,
`asset.get()` and the others) raise `RuntimeError`, because they would stop every coroutine on that
loop. Use their `_async` forms with `await`. Blocking work of your own belongs in
`await asyncio.to_thread(...)`.

If the result is cancelled (`asset.cancel()`), or the environment shuts down, the coroutine gets
`asyncio.CancelledError` at its next `await`. Let it propagate, after any cleanup; the result then
ends `Cancelled`. A function that is not `async def` but returns a coroutine fails with
`ExecutionError`.

### Errors from a command

An exception a command raises fails the query. The caller receives `lq.ExecutionError` whose
message starts with the exception's class (`ValueError: bad input`) and whose `traceback` holds the
Python stack. A command that raises a `LiquersError` keeps its type:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def lookup(key: str):
    raise lq.KeyNotFoundError(f"no record {key}", key=key)

try:
    env.evaluate("lookup-a")
except lq.KeyNotFoundError as e:
    assert e.key == "a"
else:
    raise AssertionError("expected KeyNotFoundError")
env.shutdown()
```

## 6. Running a query

| Call | Returns |
|---|---|
| `env.evaluate(query)` | the value |
| `env.evaluate_state(query)` | an `lq.State`: the value and its metadata |
| `env.get_asset(query)` | an `lq.AssetRef` at once, without waiting for the value |
| `lq.evaluate(query)` | the value, from the default environment |

`query` is a string or an `lq.Query`. A failed query raises its error from each of these calls
except `get_asset`, whose `AssetRef` raises it from `get()`.

### Reading metadata

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello(context) -> str:
    context.info("made a greeting")
    return "hello"

state = env.evaluate_state("hello")
assert state.value == "hello"
assert state.metadata.type_identifier() == "Text"
assert "made a greeting" in [entry.message for entry in state.metadata.log()]
env.shutdown()
```

`State.value` is a property, but `Metadata`'s fields are methods:

| `Metadata` method | Gives |
|---|---|
| `status()` | an `lq.Status`, such as `Ready` or `Error` |
| `is_error()`, `message()` | whether the result failed, and its last message |
| `type_identifier()` | the value's type, such as `Text` |
| `query()` | the query that produced it |
| `filename()`, `get_media_type()` | its file name and media type, when the query ends in one (see recipes, below) |
| `log()` | the log: your `info`/`warning`/`error` entries and Liquers' own, such as detected dependencies |

### Watching and cancelling

An `AssetRef` is for a result you want to watch or stop: `status` (an `lq.Status`), `metadata`,
`get()` and `get_state()` to wait for the value, and `cancel()`.

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

asset = env.get_asset("hello")
assert asset.get() == "hello"
assert asset.status == "Ready"
env.shutdown()
```

**Waiting.** `evaluate`, `evaluate_state`, `AssetRef.get` and `Context.evaluate` block until the
value is ready, with the GIL released, so other Python threads keep running. Ctrl-C interrupts a
wait with `KeyboardInterrupt`, and `shutdown()` ends any wait still outstanding with
`CancelledError`. Leaving a wait does not stop the computation; `AssetRef.cancel()` does.

**Results are reused.** A query evaluated twice is computed once while the environment keeps the
result. A command that must run every time is declared `volatile=True`.

### Awaiting a query

In async code, await the `_async` form of a call instead of blocking the event loop:

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

An `_async` call must be made while an event loop runs (inside `async def`), or it raises
`RuntimeError`. A bad query raises from the call itself, before any `await`. Cancelling the awaiting
task ends only your wait: the evaluation finishes, and its result is kept. To stop the computation,
`await asset.cancel_async()`. For a time limit, wrap the await in `asyncio.timeout(...)`. A blocking
call made inside a running event loop still works, but warns with `RuntimeWarning`.

**How many run at once.** `lq.Environment(job_capacity=8)` sets how many commands run at the same
time; the rest wait their turn. Every command shares that one limit. A command evaluating another
through `context.evaluate` or `context.evaluate_async` never waits for a slot it is itself holding,
but one calling `env.evaluate` from inside a command does, and with `job_capacity=1` it waits
forever.

### Threads and free-threaded Python

An environment can be used from several threads at once. On a free-threaded build of Python
(`python3.14t`), importing `liquers` keeps the GIL off, so sync commands called from different
threads, and async commands on different loops, really run in parallel. Two rules follow:

- A command may run on several threads at once; protect any state it shares (a `threading.Lock`
  around a counter, for example).
- Do not change one `lq.Query`, `lq.Key` or `lq.Metadata` object from two threads at once: the
  second change raises `RuntimeError`. The handles (`Environment`, `AssetRef`, `State`, `Context`)
  are read-only and safe to share.

### Named results: recipes

A *recipe* gives a query's result a key in the store, so `-R/data/triple.txt` computes it when no
file is there. Recipes live in a `recipes.yaml` in the folder they belong to; the key is the folder
plus the query's last segment, its file name:

```python
import pathlib
import tempfile
import liquers as lq

root = pathlib.Path(tempfile.mkdtemp())
(root / "data").mkdir()
(root / "data" / "recipes.yaml").write_text("recipes:\n  - query: hello/repeat-3/triple.txt\n")

env = lq.Environment(config={
    "store": {"stores": [{"type": "filesystem", "prefix": "data", "config": {"path": str(root)}}]},
})

@env.first_command
def hello() -> str:
    return "hello"

@env.command
def repeat(text: str, count: int = 2) -> str:
    return text * count

assert env.evaluate("-R/data/triple.txt") == "hellohellohello"
env.shutdown()
```

A trailing file name such as `triple.txt` also names the result and sets its media type; it can end
any query (`hello/repeat-3/triple.txt`). How recipes are resolved is in
[`reference/RECIPES_AND_PLANS.md`](../reference/RECIPES_AND_PLANS.md).

### Checking a query without running it

`lq.parse(text)` parses a query and raises `ParseError` when it is malformed; `.encode()` gives its
canonical text. `env.validate()` lists the problems the registered commands and imported metadata
would cause, as `lq.Issue` objects, without building the environment. It does not check the store
configuration, which only the first use does.

## 7. Values

Values cross between Python and Liquers by kind:

| Python | In Liquers | Back in Python |
|---|---|---|
| `None`, `bool`, `str`, `bytes` | the same | the same |
| `int` | 64-bit integer; a larger one raises `ConversionError` | `int` |
| `float` | float, including NaN and infinities | `float` |
| `bytearray` | bytes | `bytes` |
| `list`, `tuple` | array | `list` |
| `dict` with `str` keys | object | `dict` |
| `polars.DataFrame` | Polars DataFrame | `polars.DataFrame` |
| `PIL.Image.Image` | image | `PIL.Image.Image` |
| `lq.Query`, `lq.Key`, `lq.Recipe`, `lq.MetadataRecord`, `lq.CommandMetadata` | the same | the same |
| anything else | an opaque Python object | the same object |

A list or dict is converted only when everything inside it is a plain value: `None`, a bool,
number, `str` or `bytes`, a list or dict of those, or one of the `lq` objects above. One holding
anything else, a DataFrame or image included, or a dict with a non-`str` key, is kept whole as an
opaque object, so it comes back unchanged.

A DataFrame lets Python and Rust commands share a pipeline:

```python
import liquers as lq
import polars as pl

env = lq.Environment()

@env.first_command
def frame() -> pl.DataFrame:
    return pl.DataFrame({"a": [1, 2, 3]})

assert env.evaluate("frame/ns-pl/head-2").height == 2       # `head` is a Rust command
env.shutdown()
```

A DataFrame or image is copied each time it crosses, not shared.

**Opaque objects.** An object with no Liquers equivalent is kept as it is, so a later Python command
receives the same object. Liquers cannot look inside it: a Rust command given one fails with
`ConversionError`. It can be written to the store only by an environment created with
`pickle=True`; without it, writing one raises `SerializationError`, and stored pickle bytes are
never unpickled. With `conversion="strict"`, such an object is refused with `ConversionError` as
soon as a command returns it.

## 8. Errors

Every Liquers error is raised as a subclass of `lq.LiquersError`, one per `ErrorType`, named for it:
`lq.KeyNotFoundError`, `lq.ActionNotRegisteredError`, `lq.ExecutionError`. Catch
`lq.LiquersError` for any of them.

Some types also belong to categories, each with its own base class, so one `except` catches a
group: `lq.StoreError` (every store error), `lq.KeyedAssetError` (the same for keyed assets) and
`lq.NotFoundError` (also a `LookupError`). A type may be in several — `ErrorType.categories()`
lists them:

```python
import liquers as lq

env = lq.Environment()
try:
    env.evaluate("-R/data/missing.txt")
except lq.NotFoundError as e:
    # KeyNotFoundError: the store has no value there (a recipe may still make it).
    # AssetNotFoundError: nothing is stored and no recipe makes it.
    assert type(e) is lq.AssetNotFoundError and e.key == "data/missing.txt"
else:
    raise AssertionError("expected a NotFoundError")
env.shutdown()
```

| Attribute | Meaning |
|---|---|
| `type` | the `ErrorType` |
| `message` | the message |
| `query`, `key`, `position` | where it happened, when known |
| `traceback` | the stack as text: a Python command's traceback, followed by the frames it passed through on the way out of nested commands |

To raise one: `lq.KeyNotFoundError("message", key="a/b")`, or
`lq.LiquersError(lq.ErrorType.KeyNotFound, "message", key="a/b")`, which is the same.

Errors you will meet early:

| Situation | Raised |
|---|---|
| A command name that is not registered | `ActionNotRegisteredError` |
| A key that is neither in the store nor made by a recipe | `GeneralError`, "No recipe found for key …", with no `key` (`MISSING-RESOURCE-REPORTED-AS-GENERAL-ERROR`) |
| A malformed query | `ParseError` |
| An exception inside a Python command | `ExecutionError` |
| A value that cannot be converted | `ConversionError` |

The exact mapping is in [`reference/PYTHON_INTEGRATION.md`](../reference/PYTHON_INTEGRATION.md) §6.

## 9. Testing your commands

Give each test its own environment, so registrations do not leak between tests, and shut it down
at the end:

```python
import liquers as lq
import pytest

@pytest.fixture
def env():
    environment = lq.Environment()
    yield environment
    environment.shutdown()

def test_repeat(env):
    @env.command
    def repeat(text: str, count: int = 2) -> str:
        return text * count

    env.store_set("data/in.txt", "ab")
    assert env.evaluate("-R/data/in.txt/-/repeat-2") == "abab"
```

`env.store_set(key, value)` writes a value into the environment's store, which is the simplest
way to give a test its input.

## 10. Known limitations

What you will notice; the complete list, with the tracking issues, is
[`reference/PYTHON_INTEGRATION.md`](../reference/PYTHON_INTEGRATION.md) §8.

- All commands share one limit on how many run at once (`job_capacity`); a command cannot have its
  own.
- The decorators cannot set a command's realm (a deployment-level grouping of commands) or describe
  arguments beyond their annotations.
- Only the default recipe provider can be configured.
- Progress lines on stderr cannot be turned off.
- There are no type stubs for editors and no generated API reference; `help()` on a class or
  method shows its docstring and signature.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-27 | §10: the missing-docstrings limitation is lifted; type stubs and a generated reference remain. | design `py-docstrings` |
| 2026-10-27 | §5 Async commands; §6 Awaiting a query, `job_capacity`, Threads and free-threaded Python; §10 the async limitation is lifted, and the stale memory-leak line removed (fixed 2026-10-22). | `design/liquers-py-async/` (phase-5) |
| 2026-10-27 | Registration: a command named `ns`, `q` or `v` is refused (`q` and `v` used to register and never run). | design `reserved-instruction-command-names` |
| 2026-10-25 | §8 Errors: the category base classes `StoreError`, `KeyedAssetError`, `NotFoundError`. | `design/key-without-recipe-answer/` |
| 2026-10-22 | Late registration: a replaced environment is kept until `shutdown()`, no longer until exit. | `design/environment-manager-reference-cycle/` |
| 2026-10-22 | Link to the recipes reference follows its promotion to `reference/RECIPES_AND_PLANS.md`. | DOC-08 promotion |
| 2026-10-20 | `context.error` raises `ExecutionError` with the first logged message once the command returns. | `CONTEXT-ERROR-OUTCOME-DEPENDS-ON-SCHEDULING` |
| 2026-10-19 | Created from the implemented `liquers-py`. | `design/liquers-py/` (phase-5) |
