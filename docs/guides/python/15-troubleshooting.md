---
title: Troubleshooting
status: draft
sources: [guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md, reference/PROJECT_OVERVIEW.md]
reviewed: 2026-11-09
---
# 15. Troubleshooting

PURPOSE: The reader can read a Liquers error, find its cause, and knows the known limitations.

When something goes wrong, Liquers tells you what kind of problem it is and where it happened.
The first two sections explain this in the words shared by all three guides. The rest of the
chapter covers the exceptions you see in Python, the mistakes that are made most often, and what
does not work yet.

{{> ../_shared/errors.md}}

## 15.3 Exceptions in Python

Every Liquers error is raised as a subclass of `lq.LiquersError`. There is one class for each
type of section 15.1, named after it with `Error` at the end where it is not there already:
`lq.ParseError`, `lq.KeyNotFoundError`, `lq.ActionNotRegisteredError`, `lq.ExecutionError`.

```python
import liquers as lq

env = lq.Environment()
try:
    env.evaluate("-R/data/missing.txt")
except lq.LiquersError as error:
    assert type(error) is lq.AssetNotFoundError
    assert error.key == "data/missing.txt"
    print(error.type, error.message)
env.shutdown()
```

| Attribute | Holds |
|---|---|
| `type` | the type, an `lq.ErrorType` |
| `message` | the message |
| `query`, `key`, `position` | where it happened, when that is known |
| `traceback` | for an error from a Python command: the Python stack, as text |

Some classes share a base class, so that one `except` catches a group:

| Base class | Catches |
|---|---|
| `lq.LiquersError` | every Liquers error |
| `lq.NotFoundError` | `KeyNotFoundError` and `AssetNotFoundError`; it is also a `LookupError` |
| `lq.StoreError` | every error of a store |
| `lq.KeyedAssetError` | every error about an asset that has a key |

An exception that your own command raises reaches the caller as `lq.ExecutionError`, with the
class and message of your exception in `message` and the Python stack in `traceback`
(chapter 5).

## 15.4 Checking a query before running it

```python
import liquers as lq

env = lq.Environment()

try:
    lq.parse("hello world")                         # syntax only; needs no environment
except lq.ParseError as error:
    print(error.message)

try:
    env.query_editor("ns-rec/no_such_command").validate()   # checked against the commands
except lq.ActionNotRegisteredError as error:
    print(error.message)

try:
    env.query_editor("ns-rec/head-1-2").validate()          # one argument too many
except lq.TooManyParametersError as error:
    print(error.message)

assert env.validate() == []                         # the registered commands are consistent
env.shutdown()
```

A query editor made from the text of a query checks it against the commands of the environment:
the names, the namespaces and the number of arguments. Nothing is evaluated.

To see what a query means, parse it and look at its segments, as chapter 3 shows. That is the
way to find a missing `/-/`.

## 15.5 Frequent mistakes

| What you see | Likely cause |
|---|---|
| `AssetNotFoundError` for a key that ends in a command name, such as `data/x.csv/to_text` | The `/-/` between the key and the commands is missing |
| `ActionNotRegisteredError` for a library command | The `ns-…` is missing or wrong; the message lists where Liquers looked |
| `ActionNotRegisteredError` for your own command | It was registered in another environment: `@lq.command` and `env.evaluate` do not go together |
| `TooManyParametersError` | An argument contains a `-` that was not escaped as `~_` |
| `ConversionError: Can't convert … to integer` | An argument is not a number; or the arguments are in the wrong order |
| A number arrives as text | The parameter has no annotation |
| `unknown data format 'b'` from `to_record` | The bytes were written with `env.store_set`, which does not look at the extension; write them with `env.store.set`, or name the format: `to_record-csv` |
| A result does not change after the input changed | The input was changed with `env.store.set` or by another program; change it with `env.store_set` (chapter 6) |
| A command runs on every call | It, or a command before it in the query, is volatile |
| A command never runs again | It reads something Liquers cannot see change, and should be `volatile=True` |
| `GeneralError` on every call | The environment was shut down |
| `RuntimeError` in an async command | A blocking call such as `context.evaluate` was used; use the `_async` form with `await` |
| Results vanish after registering a command | Registering rebuilds the environment; register before the first evaluation |
| Lines of text on the terminal while a query runs | Progress messages on standard error; they cannot be turned off yet |

## 15.6 Known limitations

Liquers is at version 0.1. These are the gaps you are most likely to notice from Python:

- All commands share one limit on how many run at once (`job_capacity`). A single command
  cannot be given its own limit.
- The decorators describe arguments through annotations only. They cannot set a label or a
  description for an argument, or a command's *realm*.
- Every registration after the first use rebuilds the environment and drops its cached results.
- The progress messages on standard error cannot be turned off or filtered.
- The fields of `lq.Metadata` are methods (`metadata.status()`), while those of `lq.State` and
  `lq.AssetRef` are properties (`asset.status`).
- A table or a record source cannot be implemented in Python, and moving a table to or from
  pyarrow, pandas or Polars copies it.
- A result is written to the store in the background. A program that ends immediately after an
  evaluation can lose that write.
- `env.recipes.recipe(key)` for a key without a recipe raises `GeneralError`, not
  `KeyNotFoundError`; use `recipe_opt`, which returns `None`.
- There is no user-interface toolkit for Python (chapter 11), and the HTTP server is a Rust
  program (chapter 12).
- Ready-made packages exist for CPython 3.14 and later on five platforms. Elsewhere pip builds
  from source (chapter 2).

If you find a problem that is not listed here, please report it at
<https://github.com/orest-d/liquers/issues>.
