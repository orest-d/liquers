---
title: Using Liquers from Python
kind: guide
audience: user
area: [py]
reviewed: 2026-10-19
---
# Using Liquers from Python

How to use Liquers from Python with the `liquers` package: build it, configure an environment,
register commands written in Python, and evaluate queries. How the package is built on the Rust
crates, and the exact rules it follows, are in
[`reference/PYTHON_INTEGRATION.md`](../reference/PYTHON_INTEGRATION.md).

Every example below runs as written against the package built from this repository. Examples that
use `polars` need it installed.

## 1. Installing

The package is not published yet. Build it from a checkout into a virtual environment with
[maturin](https://www.maturin.rs):

```bash
python -m venv .venv && . .venv/bin/activate
pip install maturin
maturin develop -m liquers-py/Cargo.toml       # builds the extension and installs `liquers`
```

The distribution is called `liquers-py`; the package is imported as `liquers`, conventionally as
`lq`. CPython 3.9 to 3.15 and PyPy 3.11 or newer are supported. Building needs a Rust toolchain
and OpenSSL development headers (`libssl-dev`). `polars` and `pillow` are optional; they are
needed only to exchange DataFrames and images.

## 2. What is in the package

Everything is imported from `liquers`; the underscored modules are implementation, listed so a
traceback is readable.

| Module | What it holds |
|---|---|
| `liquers` | The public names, all re-exported here. |
| `liquers._native` | The Rust extension: every class below, and `parse`, `parse_key`, `build_plan`. |
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

print(lq.evaluate("commands_doc")[:10])     # "# Commands"
```

`lq.evaluate` runs a query in the process's default environment and returns its value as a Python
object. Every environment comes with the built-in commands of `liquers-lib`: `to_text`,
`commands_doc`, the Polars commands under `ns-pl`, images, records, search and templates.

A query is a path of actions: `hello/repeat-3` applies `hello`, then `repeat` with the argument
`3`. `-R/data/note.txt` reads a key from the store, and `-R/data/note.txt/-/to_text` reads it and
applies `to_text` (the `/-/` starts the actions; without it, `to_text` would be part of the key).
The query language is described in
[`reference/PROJECT_OVERVIEW.md`](../reference/PROJECT_OVERVIEW.md).

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
env.store_set("data/note.txt", "kept")
assert env.evaluate("-R/data/note.txt") == "kept"
env.shutdown()
```

`Environment` takes keyword arguments only:

| Argument | Default | Meaning |
|---|---|---|
| `config` | none | The environment configuration, as a `dict` or a YAML string: `store` (where data lives), `assets` (asset manager options). The format is [`reference/ENVIRONMENT_CONFIG.md`](../reference/ENVIRONMENT_CONFIG.md). With no `store`, the store is in memory. `recipes` must be left at `default`. |
| `conversion` | `"permissive"` | What happens to a Python object with no Liquers equivalent: `"permissive"` keeps it as an opaque object, `"strict"` refuses it with `ConversionError` (§7). |
| `pickle` | `False` | Whether opaque objects can be written to the store, as pickle. Leave it off unless you trust everything the store holds: reading pickle runs code. |

The same configuration as YAML:

```python
import liquers as lq

env = lq.Environment(config="""
assets:
  job_capacity: 2
""")
env.shutdown()
```

**Lifecycle.** An environment is built on first use, from everything declared on it, and keeps
running until `shutdown()`. `shutdown()` is idempotent; afterwards every use raises
`GeneralError`. Call it when you are done with an environment you made, typically in a `finally`
or a test fixture. An invalid `conversion` or `recipes` is refused by the constructor, but a store
that cannot be built, such as one of an unknown type, is reported by the first use.

**The default environment.** `lq.default_environment()` returns one shared environment, created on
first use with the default settings. `lq.evaluate` and the bare `@lq.command` decorators use it.

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
can still be called directly.

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
| `Optional[int]`, `Optional[float]`, `Optional[bool]` | optional; an empty or omitted argument gives the default, or `None` |
| none, or anything else | any value |

A default must be `None`, a `str`, an `int`, a finite `float` or a `bool`. `*args` takes every
remaining argument:

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
| `info(msg)`, `warning(msg)`, `error(msg)` | adds an entry to the result's log; `error` does not fail the command, raising does |
| `is_cancelled()` | whether the result was cancelled; a long command should check it and stop |

The context is valid only during the call. Keeping it and using it later raises `GeneralError`.
Put `context` before `*args`; after it, it would be keyword-only.

### Metadata

Both decorators take keyword arguments describing the command:

| Keyword | Meaning |
|---|---|
| `name` | the command's name in queries; default: the function's name |
| `namespace` | the namespace; a query selects it with `ns-<namespace>` |
| `label`, `doc` | a display name and a description; `doc` defaults to the docstring |
| `filename` | the file name the result is presented under, such as `"result.csv"` |
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
already computed are not, and the replaced environment's memory is not freed until the process
exits ([`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`](../issues/ENVIRONMENT-MANAGER-REFERENCE-CYCLE.md)).

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
env.shutdown()
```

## 6. Running a query

| Call | Returns |
|---|---|
| `env.evaluate(query)` | the value |
| `env.evaluate_state(query)` | an `lq.State`: the value and its metadata |
| `env.get_asset(query)` | an `lq.AssetRef` at once, without waiting for the value |
| `lq.evaluate(query)` | the value, from the default environment |

`query` is a string or an `lq.Query`. The metadata records how a value was made:

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

**Checking a query without running it.** `lq.parse(text)` parses a query and raises `ParseError`
when it is malformed; `.encode()` gives its canonical text. `env.validate()` lists the problems
the next build of the environment would meet, as `lq.Issue` objects, without building it.

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

A list or dict is converted only when everything inside it can be. One that holds an unconvertible
item, or a dict with a non-`str` key, is kept whole as an opaque object, so it comes back unchanged.

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
`pickle=True`. Writing one without pickle raises `SerializationError`, and an environment without
pickle never unpickles stored bytes. With `conversion="strict"`, such an object is refused with
`ConversionError` as soon as a command returns it.

## 8. Errors

Every Liquers error is raised as a subclass of `lq.LiquersError`, one per `ErrorType`:
`ErrorType.KeyNotFound` is `lq.KeyNotFoundError`, `ErrorType.ActionNotRegistered` is
`lq.ActionNotRegisteredError`, and `ErrorType.ExecutionError` is `lq.ExecutionError` (a name that
already ends in `Error` is not suffixed twice). Catch `lq.LiquersError` for any of them.

| Attribute | Meaning |
|---|---|
| `type` | the `ErrorType` |
| `message` | the message |
| `query`, `key`, `position` | where it happened, when known |
| `traceback` | the stack as text: a Python command's traceback, followed by the frames it passed through on the way out of nested commands |

To raise one: `lq.KeyNotFoundError("message", key="a/b")`, or
`lq.LiquersError(lq.ErrorType.KeyNotFound, "message", key="a/b")`, which is the same.

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

- Evaluation is synchronous; `async def` commands are not supported yet
  ([`design/liquers-py-async/`](../design/liquers-py-async/DESIGN.md)).
- The decorators cannot set a command's realm or describe its arguments beyond their annotations
  ([`PY-COMMAND-DECORATOR-CANNOT-SET-REALM`](../issues/PY-COMMAND-DECORATOR-CANNOT-SET-REALM.md),
  [`PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA`](../issues/PY-COMMAND-DECORATOR-CANNOT-DECLARE-ARGUMENT-METADATA.md)).
- Only the default recipe provider can be configured
  ([`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`](../issues/PY-RECIPE-PROVIDER-NOT-CONFIGURABLE.md)).
- An environment's memory is never freed, even after `shutdown()`
  ([`ENVIRONMENT-MANAGER-REFERENCE-CYCLE`](../issues/ENVIRONMENT-MANAGER-REFERENCE-CYCLE.md)).
- Liquers writes progress diagnostics to stderr, and they cannot be turned off yet
  ([`CORE-TRACING-MIGRATION`](../issues/CORE-TRACING-MIGRATION.md)).
- Most classes have no docstrings yet, so `help()` says little about them
  ([`PY-API-REFERENCE-NOT-GENERATED`](../issues/PY-API-REFERENCE-NOT-GENERATED.md)).

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-19 | Created from the implemented `liquers-py`. | `design/liquers-py/` (phase-5) |
