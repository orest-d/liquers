---
title: Commands
status: ready
sources: [guides/PYTHON_GUIDE.md, guides/COMMAND_REGISTRATION_GUIDE.md, reference/COMMAND_DECLARATION.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-09
---
# 5. Commands

PURPOSE: The reader can turn a Python function into a Liquers command, choose its arguments, namespace and options, and call it from a query.

A *command* is a function that a query can call. Commands are the vocabulary of your queries:
the more of them you have, the more you can say. This chapter shows how to make a command from
a Python function, starting with the simplest case and adding one feature at a time.

## 5.1 A function becomes a command

Put a decorator on a function, and it is a command:

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
assert repeat("ab", 2) == "abab"                            # still a function
env.shutdown()
```

There are two decorators, for the two places a command can have in a query:

- `@env.first_command` is for a command that *starts* a query. It makes a value from its
  arguments alone: it reads a database, generates data, returns a constant.
- `@env.command` is for a command that *continues* a query. Its first parameter receives the
  value made by the action before it.

Liquers reads the rest from the function: its name becomes the name of the command, its
parameters become the arguments, its annotations say how to convert them, and its docstring
becomes the documentation. The decorator returns the function unchanged, so you can still call
and test it as an ordinary function.

`@env.command` registers the command in the environment `env`. `@lq.command` and
`@lq.first_command` register it in the default environment, the one `lq.evaluate` uses.

## 5.2 The input

The first parameter of a `command` receives the input, and its *name* decides in what form:

| First parameter is named | It receives |
|---|---|
| `text` | the input converted to a `str` |
| `state` | an `lq.State`: the value in `.value` and its metadata in `.metadata` |
| anything else | the input value, converted to Python as chapter 4 describes |

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def answer() -> int:
    return 42

@env.command
def shout(text) -> str:                  # `text`: the input as a str
    return text.upper() + "!"

@env.command
def double(value):                       # any other name: the value itself
    return value * 2

@env.command
def describe(state) -> str:              # `state`: the value and its metadata
    return f"{state.metadata.type_identifier()} made by {state.metadata.query().encode()}"

assert env.evaluate("answer/double") == 84
assert env.evaluate("answer/shout") == "42!"
assert env.evaluate("answer/describe") == "I64 made by answer"
env.shutdown()
```

Use `text` when the command works on text, whatever the previous command produced. Use `state`
when the command needs to know something about its input, not just the value.

## 5.3 Arguments and defaults

Every other parameter is an argument. In a query, arguments follow the command's name, separated
by `-`. They arrive as text, and the annotation says what to make of it:

| Annotation | The argument is |
|---|---|
| `str` | text |
| `int`, `float` | a number; text that is not a number is refused with `ConversionError` |
| `bool` | a truth value: `t`, `true`, `yes` and `f`, `false`, `no` |
| `Optional[int]`, `Optional[float]`, `Optional[bool]` (or `int \| None`, …) | optional: an empty or missing argument gives the default, or `None` when there is none |
| none, or anything else | the text of the query, as a `str` |

```python
from typing import Optional
import liquers as lq

env = lq.Environment()

@env.first_command
def describe(count: int = 1, ratio: float = 0.5, exact: bool = False,
             limit: Optional[int] = None, label: str = "") -> list:
    return [count, ratio, exact, limit, label]

assert env.evaluate("describe") == [1, 0.5, False, None, ""]
assert env.evaluate("describe-3-0.25-t-10-total") == [3, 0.25, True, 10, "total"]
assert env.evaluate("describe-3---10") == [3, 0.5, False, 10, ""]    # empty: the default
env.shutdown()
```

Annotate every number. An argument without an annotation arrives as text, and `"3" * 2` is `"33"`.

A parameter with a default can be left out of the query. A default must be `None`, text, an
integer, a finite float or a boolean. For optional text use `label: str = ""`; `Optional[str]` is
treated as if it had no annotation.

A value that contains `-`, `/` or a space is escaped in the query (`~_`, `~/`, `~.`; chapter 3).
Your function receives the plain value.

`*args` collects all remaining arguments:

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

Keyword-only parameters and `**kwargs` cannot be given in a query, so the decorator refuses them
with `TypeError`.

## 5.4 The context

A parameter named `context` is not an argument. It receives an `lq.Context`, the command's
connection to the evaluation it is part of:

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

| `Context` method | What it does |
|---|---|
| `info(message)`, `warning(message)` | adds an entry to the log of the result |
| `error(message)` | adds an error entry *and fails the result*: the command runs on, but when it returns, the query raises `ExecutionError` with the first such message |
| `evaluate(query)` | evaluates another query and returns its value |
| `is_cancelled()` | tells whether the result was cancelled; a long command should check this now and then, and stop |
| `render(…)`, `expand(…)` | render a template (chapter 10) |

`context.evaluate` is how one command builds on other results. It does more than save you a call
to `env.evaluate`: the query it evaluates is recorded as a *dependency* of the command's result.
When that query's result changes, the command's result is out of date and is computed again
(chapter 7). So use `context.evaluate` inside commands, never `env.evaluate`.

The context is valid only while the command runs. Do not keep it. Write `context` before `*args`
if you use both.

## 5.5 Names, namespaces and other options

Both decorators take keyword arguments that describe the command:

| Keyword | Meaning |
|---|---|
| `name` | the name of the command in queries; by default the name of the function |
| `namespace` | the group the command belongs to; a query selects it with `ns-<namespace>` |
| `label`, `doc` | a short title and a description, shown in command lists; `doc` defaults to the docstring |
| `filename` | the file name offered for the result, such as `"report.csv"` |
| `volatile` | `True` for a command whose result is different every time: it is never reused |
| `cached` | `False` for a cheap command whose result is not worth keeping |
| `env` | the environment to register in: `@lq.command(env=env)` is `@env.command` |

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

Give your commands a namespace when you write a library of them. It keeps their names from
colliding with other libraries, and it lets a reader see where a command comes from. Commands
without a namespace are found from everywhere and are good for the handful of commands specific
to one application.

Declare a command `volatile=True` when it reads something Liquers cannot see change: the clock,
a random generator, a web service. Without it, the first result would be reused for ever.

Three names are taken: `ns`, `q` and `v` are instructions of the query language. A command with
one of these names is refused.

## 5.6 When to register

Register your commands first, then evaluate. The environment is built the first time it is used,
from the commands registered until then.

Registering a command later works, but it rebuilds the environment: the store is kept, and the
results computed so far are dropped. In a notebook, where you redefine a command and run it
again, that is what you want. In a service, register everything at startup.

Registering a name that exists replaces the command, with a warning that names what was
replaced. A command of the library can be replaced like this. `env.unregister(name,
namespace="")` removes a command; removing a Python command that replaced a library command
brings the library command back.

## 5.7 Async commands

A command may be an `async def`. Liquers awaits it for you. Use one for work that mostly waits,
such as a request to a web service, so that the waiting does not occupy a thread:

```python
import asyncio
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "hello"

@env.command
async def fetch_len(text: str) -> int:
    await asyncio.sleep(0.01)          # stands in for an HTTP request
    return len(text)

@env.first_command
async def outer(context) -> str:
    return f"length {await context.evaluate_async('hello/fetch_len')}"

assert env.evaluate("hello/fetch_len") == 5
assert env.evaluate("outer") == "length 5"
env.shutdown()
```

The coroutine runs on an event loop that belongs to the environment, not on yours. So an async
command can be used from ordinary code, as above, and a query is the same whether its commands
are async or not.

Inside an async command, use the awaitable forms: `await context.evaluate_async(…)` instead of
`context.evaluate(…)`. The blocking forms raise `RuntimeError` there, because they would stop
every other coroutine on the loop. Put blocking work of your own into
`await asyncio.to_thread(…)`.

When the result is cancelled or the environment shuts down, the coroutine receives
`asyncio.CancelledError` at its next `await`. Clean up if you need to, and let the exception
pass on.

## 5.8 Errors from a command

An exception raised in a command fails the query. The caller gets an `lq.ExecutionError`. Its
message starts with the class of your exception (`ValueError: bad input`), and its `traceback`
attribute holds the Python stack:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def half(n: int) -> int:
    if n % 2:
        raise ValueError(f"{n} is odd")
    return n // 2

try:
    env.evaluate("half-3")
except lq.ExecutionError as error:
    assert error.message.startswith("ValueError: 3 is odd")
    assert "half" in error.traceback
env.shutdown()
```

To report a specific kind of Liquers error, raise it. It keeps its type on the way to the
caller:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def lookup(key: str):
    raise lq.KeyNotFoundError(f"no record {key}", key=key)

try:
    env.evaluate("lookup-a")
except lq.KeyNotFoundError as error:
    assert error.key == "a"
env.shutdown()
```

## 5.9 Commands without a function

Not every command needs code. Two kinds are defined by what they are made of.

A *pipeline command* is a command made of other commands. You give it a name, the steps, and the
arguments that the steps use:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def top(value, n: int = 1) -> list:
    return sorted(value, reverse=True)[:n]

env.pipeline_command(
    "best",
    [{"action": "top", "parameters": [{"placeholder": "n"}]}],
    arguments=["n"],
)
assert env.evaluate("numbers/best-1") == [3]
env.shutdown()
```

`best-1` runs `top` with `n` set to 1. A pipeline command is a way to give a name to a sequence
of steps that you use often, and to hide the steps from the people who use the name.

A *template command* is a command whose result is a rendered template. Chapter 10 describes it.
