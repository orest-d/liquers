---
title: Values, state and metadata
status: draft
sources: [reference/VALUE_TYPE_SYSTEM.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-09
---
# 4. Values, state and metadata

PURPOSE: The reader knows what a command receives and returns in Python, how values convert between Python and Liquers, and what metadata travels with a value.

A query passes a value from command to command. Some of these commands are Python functions and
some are part of the library, which is written in Rust. This chapter explains what happens to a
value on the way, and what else travels with it.

## 4.1 A value and its metadata

`evaluate` gives you the value. `evaluate_state` gives you a *state*: the value together with its
*metadata*, the record of what the value is and how it was made.

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def hello(context) -> str:
    context.info("made a greeting")
    return "hello"

state = env.evaluate_state("hello")
assert state.value == "hello"

metadata = state.metadata
assert metadata.type_identifier() == "Text"
assert str(metadata.status()) == "Ready"
assert metadata.query().encode() == "hello"
assert "made a greeting" in [entry.message for entry in metadata.log()]
env.shutdown()
```

`state.value` is a property. The fields of the metadata are methods:

| `Metadata` method | Gives |
|---|---|
| `status()` | an `lq.Status`, such as `Ready` or `Error` (chapter 7) |
| `type_identifier()` | the type of the value as Liquers names it: `Text`, `I64`, `RecordView`, … |
| `query()` | the query that made the value |
| `log()` | the log, a list of `lq.LogEntry` with `kind`, `message` and `timestamp` |
| `is_error()`, `message()` | whether the evaluation failed, and its last message |
| `filename()`, `get_media_type()` | the file name and media type, when the query ends in a file name |
| `to_json()` | everything, as JSON text |

The log holds what your commands wrote with `context.info` and `context.warning`, and what
Liquers added: for example, one line for each dependency it detected.

## 4.2 How values cross between Python and Liquers

When a Python command returns a value, Liquers converts it to one of its own types, so that the
next command can use it whatever language it is written in. When a value reaches Python, it is
converted back. The conversion goes by the kind of the value:

| Python | In Liquers | Back in Python |
|---|---|---|
| `None`, `bool`, `str`, `bytes` | the same | the same |
| `int` | a 64-bit integer; a larger one raises `ConversionError` | `int` |
| `float` | a float, including NaN and the infinities | `float` |
| `bytearray` | bytes | `bytes` |
| `list`, `tuple` | an array | `list` |
| `dict` with `str` keys | an object | `dict` |
| `polars.DataFrame` | a Polars data frame | `polars.DataFrame` |
| `PIL.Image.Image` | an image | `PIL.Image.Image` |
| `lq.RecordView`, `pyarrow.Table` | a table (chapter 9) | `lq.RecordView` |
| `lq.Query`, `lq.Key`, `lq.Recipe` | the same | the same |
| anything else | an opaque Python object | the same object |

Three details are worth remembering.

A `tuple` comes back as a `list`, and a `bytearray` as `bytes`. Liquers has one type for each
kind of thing.

A list or a dictionary is converted only when everything inside it is plain: `None`, booleans,
numbers, text, bytes, and lists and dictionaries of those. A list that holds a data frame, or a
dictionary with a key that is not a `str`, is not converted. It is kept whole as an opaque object
and comes back unchanged.

A data frame or an image is copied each time it crosses between Python and Liquers. For a large
table that passes through many Python commands, the copies add up; chapter 9 shows how to read a
table without copying it.

## 4.3 Objects Liquers does not know

An object with no Liquers equivalent, such as an instance of your own class, a `datetime` or a
fitted model, is kept as it is. The next Python command receives the very same object:

```python
import liquers as lq

class Model:
    def __init__(self, slope):
        self.slope = slope

env = lq.Environment()

@env.first_command
def fit() -> Model:
    return Model(slope=2.0)

@env.command
def predict(model, x: float) -> float:
    return model.slope * x

assert env.evaluate("fit/predict-3") == 6.0
assert isinstance(env.evaluate("fit"), Model)
env.shutdown()
```

Such an *opaque object* has limits, because Liquers cannot look inside it:

- A command of the library that is given one fails with `ConversionError`.
- It cannot be written to a store, so it lives only as long as the environment. Writing one
  raises `SerializationError`.

The second limit can be lifted with `lq.Environment(pickle=True)`, which stores opaque objects
with `pickle`. Turn it on only when you trust everything in the store: reading a pickle runs
code.

If you would rather be told when a command returns something Liquers cannot convert, create the
environment with `conversion="strict"`. An opaque object is then refused with `ConversionError`
as soon as a command returns it.

## 4.4 What a stored file becomes

A query that starts from a file gets the content of the file, and the extension of the key
decides what kind of value that is:

```python
import liquers as lq

env = lq.Environment()
env.store.set("data/note.txt", b"remember the milk")
env.store.set("data/settings.json", b'{"limit": 3}')
env.store.set("data/sales.csv", b"region,amount\nEU,120\n")

assert env.evaluate("-R/data/note.txt") == "remember the milk"       # text
assert env.evaluate("-R/data/settings.json") == {"limit": 3}         # parsed
assert env.evaluate("-R/data/sales.csv") == b"region,amount\nEU,120\n"   # bytes
env.shutdown()
```

Text files arrive as `str`, JSON and YAML files as the data they describe, and everything else
as `bytes`. A CSV file is bytes until a command reads it as a table: `ns-rec/to_record` or
`ns-pl/from_csv` (chapter 9).

## 4.5 Reading a value field by field

Sometimes you want one field of a large value and not the whole value converted. A *data node*
is a read-only view that reads a value one field at a time:

```python
import datetime
import liquers as lq

node = lq.data_node({"name": "Ann", "when": datetime.datetime.now(), "tags": ["a", "b"]})
assert node["name"].scalar == "Ann"
assert node["tags"].kind == lq.NodeKind.List and len(node["tags"]) == 2
assert node["when"].kind == lq.NodeKind.Opaque          # no Liquers equivalent
assert node.get("missing") is None
```

`lq.data_node` accepts a Python value or a state. Given a state, it reads the value where it is,
without converting it. The first cell of a table is then `lq.data_node(state)[0]["amount"].scalar`,
and polars need not even be installed. Templates read their data in the same way (chapter 10),
which is why a template can render a dictionary that holds a `datetime`.

Dictionaries, lists, tuples and plain values can be read like this without preparation. Liquers
never runs the code of a class it does not know, so an object of your own class is opaque until
the class opts in:

```python
import liquers as lq

class Person:
    __liquers_data__ = True                 # read through keys() and __getitem__
    def keys(self):
        return ["name"]
    def __getitem__(self, key):
        if key == "name":
            return "Ann"
        raise KeyError(key)

class Wrapper:
    def __init__(self, data):
        self.data = data
    def __liquers_data__(self):             # or: return the data to read instead
        return self.data

assert lq.data_node(Person())["name"].scalar == "Ann"
assert lq.data_node(Wrapper({"a": 1}))["a"].scalar == 1
```

With `__liquers_data__ = True`, an object is read as a dictionary through `keys` and
`__getitem__`, as a list through `__len__` and `__getitem__`, or as a number through `__index__`
or `__float__`.

A folder of the store and a table that is read in chunks are read on demand, so their nodes are
used with `await`. Make such a node with `env.data_node(state)`, or `context.data_node(state)`
inside a command, and use `await node.get_async(name)` and `await node.to_json_async()`.
