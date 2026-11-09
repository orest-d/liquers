---
title: The environment
status: draft
sources: [reference/ENVIRONMENT_CONFIG.md, guides/PYTHON_GUIDE.md, reference/PYTHON_INTEGRATION.md]
reviewed:
---
# 8. The environment

PURPOSE: The reader can build an environment with the commands, stores and recipes an application needs.

An *environment* holds everything a query needs: the commands, the store, the recipes and the
asset manager that runs evaluations. A script can use the default one, which `@lq.command`
registers in and `lq.evaluate` uses. Anything else, a test, an application, a store on disk, makes
its own `lq.Environment`.

## 8.1 Making an environment

```python
import liquers as lq

env = lq.Environment()

@env.first_command
def greet(name: str = "world") -> str:
    return f"Hello, {name}!"

print(env.evaluate("greet-Ann"))        # Hello, Ann!
env.shutdown()
```

The environment is built on first use, from the commands registered on it. Registering a command
later is fine: the next use rebuilds it and keeps the store's data. Call `env.shutdown()` when you
are done with an environment you made; afterwards every use raises.

TBD: 8.1 continued: the stores and recipe providers written in Python, validation, the default environment.
SOURCE: guides/PYTHON_GUIDE.md §4

## 8.2 Configuring from one file

One file configures the whole environment: where data is stored, where recipes come from, how
assets are kept, the command metadata, the web server, and this package's own settings. Pass it as
a `pathlib.Path`:

```yaml
# liquers.yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: ./data}}
assets:
  job_capacity: 4
command_registry: commands.yaml
python:
  pickle: false
server:
  port: 8000
```

```python
import pathlib
import liquers as lq

env = lq.Environment(config=pathlib.Path("liquers.yaml"))
```

The file may be YAML (`.yaml`, `.yml`) or JSON (`.json`). You can also pass the same content as a
`dict`, or as YAML text in a `str`. A `str` is always the text, never a file name:
`lq.Environment(config="liquers.yaml")` raises and tells you to use `pathlib.Path`. An empty file,
`{}` and no `config` at all are the same: every default.

| Section | What it configures |
|---|---|
| `store` | where data lives; without it, the store is in memory ([6. Stores](06-stores.md)) |
| `recipes` | where recipes come from |
| `assets` | the asset manager: how many commands run at once, what is kept |
| `default_namespaces` | namespaces an unqualified command is also looked up in |
| `command_registry` | a command metadata file (8.4) |
| `python` | this package's settings: `conversion`, `pickle`, `async_loops`; leave `pickle` off unless you trust everything the store holds, because reading pickle runs code |
| `server` | the web server ([12. Serving and embedding](12-serving.md)) |

A section Liquers does not know is an error that names it and lists the known ones, so a typo such
as `sever:` never goes unnoticed. The same holds for a misspelled key inside `python` and
`server`. A relative file name in the document, such as `command_registry`, is found next to the
document; store paths are relative to the working directory.

YAML can use environment variables. A store path written like this takes the value of
`LIQUERS_DATA` when the store is built, and an unset variable is an error:

```yaml
store:
  stores:
    - {type: filesystem, prefix: data, config: {path: ${LIQUERS_DATA}}}
```

## 8.3 Keywords override the file

`lq.Environment` also takes keywords: `conversion`, `pickle`, `async_loops` and `job_capacity`.
A keyword you give wins over the file, and the file wins over the default:

```python
env = lq.Environment(config=pathlib.Path("liquers.yaml"), job_capacity=2)
```

This environment uses every setting of the file except `assets.job_capacity`, which is 2. So one
file can serve a laptop and a server that differ in a setting or two. In the same way, a keyword
of `env.serve()` overrides the file's `server` section.

## 8.4 Command metadata from a file

A *command metadata file* describes commands your code registers: their labels, documentation,
argument names and hints for user interfaces. It never registers a command; it only describes
the ones you register. Name it in the configuration:

```yaml
command_registry: commands.yaml
```

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
`"Greet someone"`. The file is read when the environment is made, so a missing or malformed file
raises at once, naming it. A file entry naming a command nobody registered is reported by
`env.validate()` and fails the next build.

The same in Rust: [8. The environment](../rust/08-environment.md).
