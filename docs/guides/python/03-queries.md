---
title: Queries
status: ready
sources: [reference/PROJECT_OVERVIEW.md, guides/QUERY_ESCAPING_GUIDE.md, guides/PYTHON_GUIDE.md, reference/PIPELINES.md, guides/PIPELINE_GUIDE.md]
reviewed: 2026-11-14
---
# 3. Queries

PURPOSE: The reader can read and write a query, run one from Python, and build and edit one from values without escaping by hand.

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

## 3.9 Building and editing a query

Writing a query by hand is fine when you know its text. When part of it comes from a variable,
a user's input for example, do not paste the value into the text. A value containing `-`, `/` or
a space would change the meaning of the query, and escaping it by hand is easy to get wrong.

Use a *query editor* instead. It builds the query step by step, takes each argument as the
Python value it is, writes it into the query correctly, and checks each step against the
commands of the environment:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def top(value, n: int = 1, descending: bool = True, label: str = "") -> list:
    picked = sorted(value, reverse=descending)[:n]
    return [f"{label}{x}" for x in picked] if label else picked

editor = env.query_editor()
editor.action("numbers")
step = editor.action("top")
step.set("n", 2)                              # a number goes in as a number
step.set("descending", False)                 # a boolean as a boolean
step.set("label", "no. 1-")                   # a text as it is; the editor escapes it

query = editor.query()
assert query.encode() == "numbers/top-2-false-no.~.1~_"
assert env.evaluate(query) == ["no. 1-1", "no. 1-2"]

try:
    step.set("count", 3)                      # top has no such argument
except lq.ParameterError:
    pass
assert editor.query().encode() == query.encode()      # a refused edit changes nothing
env.shutdown()
```

Pass numbers and booleans as they are, not as text. You do not need to know how a query
spells `False` or a negative number; the editor does. `set` takes the name of an argument and
puts the value in the right position. `step.push(value)` appends the next positional argument
instead, which is what you use for a command that takes any number of arguments.

The editor knows the commands that were registered when it was created. A wrong command name or
argument name raises `ParameterError` at once and leaves the editor unchanged, so mistakes show
up where they are made and not later, when the query runs.

The same editor changes a query that already exists. Give `query_editor` the query, and edit
its steps by their position:

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def top(value, n: int = 1, descending: bool = True) -> list:
    return sorted(value, reverse=descending)[:n]

editor = env.query_editor("numbers/top-1")
step = editor.edit_action(1)                  # steps are counted from 0
step.set("n", 3)
step.set("descending", False)
assert editor.query().encode() == "numbers/top-3-false"

step.unset("descending")                      # back to the default
assert editor.query().encode() == "numbers/top-3"
assert [a.name for a in editor.unset_arguments(1)] == ["descending"]

editor.set_filename("top.json")
assert editor.query().encode() == "numbers/top-3/top.json"
editor.remove(1)                              # drop the step
editor.set_filename(None)
assert editor.query().encode() == "numbers"
env.shutdown()
```

`editor.action(name)` appends a step and `editor.resource(key)` appends the reading of a key;
`insert`, `replace`, `remove` and `move_to` change the steps that are there.
`editor.command_metadata(i)`, `editor.argument_info(i, name)` and `editor.unset_arguments(i)`
describe the command of a step and its arguments, which is what a form that edits a query needs
to show.

There is an older, lower-level way, which needs no environment and checks nothing:
`lq.ActionRequest.from_arguments` builds one action from text values and escapes them.

```python
import liquers as lq

action = lq.ActionRequest.from_arguments("greet", ["Good morning", "a-b/c"])
assert action.encode() == "greet-Good~.morning-a~_b~/c"
```

It takes text only, so a number or a boolean has to be turned into its query spelling by you,
and it cannot tell you that the command has no such argument. Use the editor unless you have no
environment at hand.

API reference: [`Environment.query_editor`](https://orest-d.github.io/liquers/api/python/liquers.html#Environment.query_editor),
[`QueryEditor`](https://orest-d.github.io/liquers/api/python/liquers.html#QueryEditor) and [`ActionEditor`](https://orest-d.github.io/liquers/api/python/liquers.html#ActionEditor), the object that `action`
and `edit_action` return.

## 3.10 Pipelines: queries as data

{{> ../_shared/pipelines.md}}

### Running a pipeline from Python

In Python a pipeline is a list. Each step is a string of query text or a dictionary in the form
of the table above, so a pipeline can be written directly in code:

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

`evaluate` and `evaluate_state` take a pipeline wherever they take a query. The argument
`options` is a dictionary, which no query could carry.

`lq.Pipeline` is the pipeline as an object. It reads and writes the JSON form, and converts to
and from a query:

```python
import json
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def top(value, n: int = 1, descending: bool = True) -> list:
    return sorted(value, reverse=descending)[:n]

@env.command
def pick(value, options=None, n: int = 1) -> list:
    return sorted(value, reverse=options["descending"])[:n]

pipeline = lq.Pipeline(["numbers", {"action": "top", "arguments": {"n": 2, "descending": False}}])
assert env.evaluate(pipeline) == [1, 2]
assert pipeline.to_query(env).encode() == "numbers/top-2-false"       # it has a query form

# Every query has a pipeline form; this is how to look inside a query.
steps = json.loads(lq.Pipeline.from_query("numbers/top-2").to_json())["elements"]
assert [step["action"] for step in steps] == ["numbers", "top"]
assert steps[1]["parameters"][0]["value"] == "2"

# A pipeline with a dictionary as an argument has no query form.
structured = lq.Pipeline(["numbers", {"action": "pick", "arguments": {"options": {"descending": True}}}])
try:
    structured.to_query(env)
except lq.ParameterError as error:
    assert "not representable as a query" in error.message
env.shutdown()
```

`to_query` needs the environment, because it has to know the order of a command's arguments to
write named ones by position. `to_json()` and `Pipeline.from_json(text)` are the form in which
a pipeline is stored or sent; a pipeline made from a parsed query also records the position of
each step in the text, as `pos`.

### Editing a pipeline

`env.pipeline_editor()` is the query editor's sibling. It has the same methods, and it also
accepts what a query cannot hold:

```python
import json
import liquers as lq

env = lq.Environment()

@env.first_command
def numbers() -> list:
    return [3, 1, 2]

@env.command
def pick(value, options=None, n: int = 1) -> list:
    return sorted(value, reverse=options["descending"])[:n]

editor = env.pipeline_editor()
editor.action("numbers")
step = editor.action("pick")
step.set("options", {"descending": True})     # refused by a query editor
step.set("n", 2)
assert env.evaluate(editor.pipeline) == [3, 2]

editor.edit_action(1).set("n", 1)
assert env.evaluate(editor.pipeline) == [3]
assert json.loads(editor.pipeline.to_json())["elements"][1]["arguments"]["n"] == 1
env.shutdown()
```

The difference between the two editors is what they promise. A query editor refuses every edit
after which the result would no longer be a query, so its `query()` always succeeds. A pipeline
editor accepts those edits, and `editor.pipeline` is then a pipeline without a query form.
`env.pipeline_editor(pipeline)` starts from an existing pipeline, a list or a query.

{{> ../_shared/pipeline-or-query.md}}

In Python that means: `env.evaluate(pipeline)` computes every time, and `env.get_asset` raises
a `TypeError` for a pipeline. Chapter 7 shows a pipeline in a recipe, and section 5.9 a pipeline
registered as a command with `env.pipeline_command`.

API reference: [`Pipeline`](https://orest-d.github.io/liquers/api/python/liquers.html#Pipeline), [`PipelineEditor`](https://orest-d.github.io/liquers/api/python/liquers.html#PipelineEditor) and
[`Environment.pipeline_editor`](https://orest-d.github.io/liquers/api/python/liquers.html#Environment.pipeline_editor).

