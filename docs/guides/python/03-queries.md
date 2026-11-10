---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-09
---
# 3. Queries

PURPOSE: The reader can read and write a query, run one from Python, and build one from values without escaping by hand.

The query is the one thing in Liquers that you cannot do without, and the one thing that is the
same in every language. This chapter first describes the query language itself, in the words
shared by all three guides. Sections 3.7 to 3.10 then show how to work with queries from Python.

The examples of the first sections use small commands of your own, `hello`, `greet` and `repeat`,
like those of chapters 1 and 5, and a few commands of the library.

{{> ../_shared/queries.md}}

## 3.7 Running a query

`evaluate` takes a query and returns its value as a Python object:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "world"

@env.command
def greet(text: str, greeting: str = "Hello") -> str:
    return f"{greeting}, {text}!"

assert env.evaluate("hello/greet-Hi") == "Hi, world!"
assert env.evaluate("hello/greet-Good~.morning") == "Good morning, world!"
env.shutdown()
```

There are three ways to ask for a result, depending on how much you want to know about it:

| Call | Returns | Use it when |
|---|---|---|
| `env.evaluate(query)` | the value | you want the result |
| `env.evaluate_state(query)` | an `lq.State`: the value and its metadata | you also want to know its type, its log or how it was made (chapter 4) |
| `env.get_asset(query)` | an `lq.AssetRef`, at once | the computation is long and you want to watch or cancel it (chapter 7) |

`lq.evaluate(query)` is `evaluate` on the default environment. The first two calls wait until the
value is there. A query that fails raises an exception; chapter 15 describes them. In code that
uses `asyncio`, each call has a form to `await`: `await env.evaluate_async(query)` (chapter 8).

## 3.8 Looking inside a query

`lq.parse` turns the text of a query into an `lq.Query` object without running anything. It is
the quickest check that a query is well formed, and it lets you take a query apart:

```python
import liquers as lq

query = lq.parse("-R/data/sales.csv/-/ns-pl/from_csv/head-2/top.csv")

assert query.filename() == "top.csv"
assert query.extension() == "csv"
assert [segment.encode() for segment in query.segments] == [
    "-R/data/sales.csv",
    "-/ns-pl/from_csv/head-2/top.csv",
]
assert query.encode() == "-R/data/sales.csv/-/ns-pl/from_csv/head-2/top.csv"

try:
    lq.parse("hello world")
except lq.ParseError as error:
    print(error.message)        # Can't parse query 'hello world': ...
```

`encode()` gives the text of a query back, in its canonical form. Every function that takes a
query accepts either the text or an `lq.Query`.

A key has its own type, `lq.Key`, and its own parser:

```python
import liquers as lq

key = lq.parse_key("data/sales.csv")
assert str(key) == "data/sales.csv"
assert str(key.parent()) == "data"
assert key.filename() == "sales.csv"
```

Parsing checks the syntax only. Whether the commands exist and take these arguments is checked
when the query is planned, which happens when you evaluate it. A query with an unknown command
or a surplus argument fails then, before any command runs.

## 3.9 Building a query from values

Writing a query by hand is fine when you know its text. When part of it comes from a variable,
a user's input for example, do not paste the value into the text. A value containing `-`, `/` or
a space would change the meaning of the query, and escaping it by hand is easy to get wrong.

Use a *query editor* instead. It builds the query step by step, escapes every value, and checks
each step against the commands of the environment:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello() -> str:
    return "world"

@env.command
def greet(text: str, greeting: str = "Hello") -> str:
    return f"{greeting}, {text}!"

user_input = "Good morning: a-b/c"

editor = env.query_editor()
editor.action("hello")
step = editor.action("greet")
step.set("greeting", user_input)             # by name; the value goes in as it is

query = editor.query()
assert query.encode() == "hello/greet-Good~.morning~ncolon~~.a~_b~/c"
assert env.evaluate(query) == "Good morning: a-b/c, world!"

try:
    step.set("salutation", "Hi")              # greet has no such argument
except lq.ParameterError:
    pass
env.shutdown()
```

The editor knows the commands that were registered when it was created. A wrong command name or
argument name raises `ParameterError` at once and leaves the editor unchanged, so mistakes show
up where they are made and not later, when the query runs.

For a single action without an environment, `lq.ActionRequest.from_arguments` does the escaping:

```python
import liquers as lq

action = lq.ActionRequest.from_arguments("greet", ["Good morning", "a-b/c"])
assert action.encode() == "greet-Good~.morning-a~_b~/c"
```

## 3.10 Pipelines: queries as data

Some values cannot be written into a query at all: a dictionary, a list, a long text. For these
there is a second form of a query, the *pipeline*. A pipeline is the same chain of steps, written
as data instead of text: a list in which each step is either query text or a dictionary that
names an action and its arguments.

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def pick(value, options=None, n: int = 1) -> list:
    return sorted(value, reverse=options["descending"])[:n]

steps = [
    "numbers",
    {"action": "pick", "arguments": {"options": {"descending": True}, "n": 2}},
]
assert env.evaluate(steps) == [3, 2]         # the dictionary arrives whole
env.shutdown()
```

`evaluate` and `evaluate_state` take a pipeline wherever they take a query. A pipeline can be
written as JSON or YAML, so it is also the form in which another program can send you a
computation.

Every query can be turned into a pipeline, and a pipeline can be turned back into a query when
all its arguments can be written as text:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def top(value, n: int = 1) -> list:
    return sorted(value, reverse=True)[:n]

pipeline = lq.Pipeline(["numbers", {"action": "top", "arguments": {"n": 2}}])
assert pipeline.to_query(env).encode() == "numbers/top-2"
assert lq.Pipeline.from_query("numbers/top-2").to_query(env).encode() == "numbers/top-2"
assert env.evaluate(pipeline) == [3, 2]
env.shutdown()
```

Prefer a query when one can say what you mean: it is shorter, it is a link, and it can be used
in a recipe and in a URL. Use a pipeline when an argument is structured data. `get_asset` takes
queries only.
