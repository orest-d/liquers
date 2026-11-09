---
title: Installing and setting up
status: draft
sources: [guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed: 2026-11-09
---
# 2. Installing and setting up

PURPOSE: The reader has Liquers installed for Python and knows which optional parts exist.

The distribution is called `liquers-py`. The package it installs is imported as `liquers`, by
convention under the short name `lq`.

## 2.1 Installing the package

CHECK: no release is on PyPI yet. Until the first release (`liquers-py-v0.1.0`), `pip install liquers-py` does not work, and section 2.3 is the only way to install. Remove this note when the first release is published.

```bash
pip install liquers-py
```

Check the installation:

```python
import liquers as lq

print(lq.__version__)
print(lq.evaluate("commands_doc")[:10])     # "# Commands"
```

Liquers supports CPython 3.10 to 3.15 and PyPy 3.11 or newer.

Ready-made packages (wheels) exist for CPython 3.14 and later, both the regular and the
free-threaded build, on these platforms:

| System | Processors |
|---|---|
| Linux (glibc) | x86_64, aarch64 |
| macOS | arm64 (Apple silicon) |
| Windows | x86_64, arm64 |

On any other combination, such as CPython 3.10 to 3.13, PyPy, macOS on Intel or Alpine Linux, pip
builds the package from its source. That needs a Rust toolchain (version 1.95 or newer; install
it from <https://rustup.rs>) and a C compiler, and it takes several minutes. If you would rather
have pip fail than build, add `--only-binary :all:`.

## 2.2 Optional packages

Liquers itself has no Python dependencies. Some features hand you objects of other packages, and
need those packages installed. They are grouped into *extras*:

| Extra | Installs | You need it for |
|---|---|---|
| `polars` | polars | data frames as `polars.DataFrame` |
| `images` | pillow | images as PIL images |
| `arrow` | pyarrow | tables to and from `pyarrow.Table` |
| `pandas` | pyarrow, pandas | tables to and from `pandas.DataFrame` |
| `all` | all of the above | |

```bash
pip install 'liquers-py[polars,images]'
pip install 'liquers-py[all]'
```

You can install these packages yourself instead; Liquers uses whatever it can import. To find
out in a program whether a feature is available, ask for the extra by name:

```python
import liquers as lq

try:
    lq.require("arrow")
except ModuleNotFoundError as error:
    print(error)            # says which pip command adds it
```

Two platforms have gaps. pyarrow has no wheel for Windows on arm64, so the `arrow` and `pandas`
extras install nothing there. polars has no wheel for free-threaded Python: on such an
interpreter leave out `polars` and `all`, and name the other extras
(`'liquers-py[images,arrow,pandas]'`).

## 2.3 Building from source

To use the current state of the repository, or to install before the first release, build the
package with [maturin](https://www.maturin.rs). You need a Rust toolchain (1.95 or newer) and a
C compiler.

```bash
git clone https://github.com/orest-d/liquers.git
cd liquers
python -m venv .venv
. .venv/bin/activate
pip install maturin
maturin develop --release -m liquers-py/Cargo.toml
```

`maturin develop` compiles the extension and installs `liquers` into the active virtual
environment. The first build takes a while; later ones only compile what changed.

## 2.4 What is in the package

Everything you use is imported from `liquers`. The names fall into a few groups, and each group
has its chapter:

| For | Names | Chapter |
|---|---|---|
| Running queries | `evaluate`, `Environment`, `default_environment`, `State`, `AssetRef` | 3, 7, 8 |
| Writing commands | `command`, `first_command`, `Context`, `link` | 5 |
| The query language | `parse`, `parse_key`, `Query`, `Key`, `Pipeline`, `QueryEditor` | 3 |
| Results and their metadata | `Metadata`, `Status`, `LogEntry`, `DataNode`, `data_node` | 4 |
| Stores | `Store` | 6 |
| Recipes | `Recipe`, `Expires` | 7 |
| Tables | `RecordView`, `RecordSource`, `RecordSchema`, `FieldType` | 9 |
| Templates | `RenderResult`, `Diagnostic` | 10 |
| Errors | `LiquersError` and one subclass for each kind of error | 15 |
| The package | `require`, `__version__` | 2 |

The complete list, with every method, is the
[API reference](https://orest-d.github.io/liquers/api/python/liquers.html). The same text is
available in Python through `help(lq.Environment)`, and the package ships type declarations, so
editors and type checkers know the signatures.

## 2.5 Things to know before you start

**Progress messages.** While Liquers evaluates a query, it writes progress lines to the standard
error stream. They cannot be turned off yet. Standard output is never written to, so the output
of your own program stays clean. In a terminal you can hide the messages with `2>/dev/null`.

**The default environment.** `lq.evaluate` and the decorators `@lq.command` and
`@lq.first_command` work on one shared environment, which is created the first time it is needed
and keeps its data in memory. That is convenient for a script or a notebook. For anything that
needs its own storage or its own set of commands, including tests, create an environment
yourself (chapter 8).

**Version 0.1.** The interfaces described in this guide can still change between releases.
