# Phase 1: High-Level Design — py-command-line-config

## Purpose

Let a program start the usual way: add the Liquers options to its own `argparse` parser, pick the
configuration file (named, or a default), apply command-line overrides such as `--port` and
`--command-registry`, and configure the environment. It works step by step or in one call (Orest,
2026-11-14).

## Problem Example

```python
# app.py  —  run as: python app.py --config prod.yaml --port 9000
import argparse
import liquers as lq
import app_commands                       # registers commands with @lq.command

parser = argparse.ArgumentParser()
parser.add_argument("--title", default="My app")      # the program's own option
lq.add_arguments(parser)                  # today: no such function
args = parser.parse_args()
lq.configure_from_args(args)              # prod.yaml, then the port overridden
lq.serve(background=False)                # should serve on port 9000

# or, in one call:
args = lq.configure_from_command_line(parser)
```

Today a program writes its own options, cannot override `server:` or `command_registry` without
losing the file's folder, and cannot configure the default environment.

## Scope and Acceptance Criteria

- **AC-1** Liquers options on the program's parser
  WHEN a program calls `lq.add_arguments(parser)` on its `argparse.ArgumentParser`
  THEN the parser gains a "Liquers" group with the options of open question 1, the program's own
  arguments are untouched, and `--help` lists both. WHEN the program already defines one of those
  option names
  THEN `add_arguments` raises `argparse.ArgumentError`, and `options=[…]` leaves that option out.
- **AC-2** Configure from parsed arguments
  WHEN a program run with `--config app.yaml --port 9000 --command-registry cmr.yaml` calls
  `lq.configure_from_args(args)`
  THEN the default environment is configured from `app.yaml` with `server.port` 9000 and the
  registry `cmr.yaml`, and `lq.serve()` listens on 9000. An option not given, a boolean included,
  leaves the file's value. `configure_from_args(args, env=env)` configures `env`.
- **AC-3** The default configuration file
  WHEN `--config` is not given
  THEN `liquers.yaml` in the working directory is used if it exists, else the built-in defaults.
  WHEN `add_arguments(parser, default_config=None)` was used
  THEN no file is read. WHEN `--config` names a missing file
  THEN `configure_from_args` raises `lq.ConfigurationError` naming it.
- **AC-4** Paths keep their meaning
  WHEN the file's `command_registry:` is relative and `--command-registry` is not given
  THEN it resolves against the file's folder, as today. WHEN `--command-registry` is given
  THEN it resolves against the working directory.
- **AC-5** A single call
  WHEN a program calls `args = lq.configure_from_command_line(parser=None, argv=None)`
  THEN the options are added, `argv` (default `sys.argv[1:]`) is parsed, the environment is
  configured as in AC-2, and the namespace is returned with the program's own options.
- **AC-6** The guide's UI and server examples load a YAML configuration
  WHEN a user-guide example starts a server (chapter 12) or a UI (chapter 11)
  THEN it loads the environment's configuration from a YAML file, and chapter 8 shows the
  step-by-step form and the single call as runnable examples.

Non-goals: an environment variable naming the file (deferred by Orest, 2026-11-14); command-line
options for the store and the recipes, whose structure does not fit an option; a generic
`--set section.key=value`, unless open question 1 adds it.

## Core Interactions

- Configuration: a new merge of a partial document over a parsed one (`ConfigDocument::overlay`).
- Bindings: `Environment(...)` and `configure(...)` gain `overrides=`, and a new pure-Python
  `argparse` layer builds that overlay.
- Server and UI: unchanged. They read the configured settings.

## Crate Placement

`liquers-py` (the `argparse` layer, and `config.rs`) and `liquers-core` (`environment_config.rs`:
`ConfigDocument::overlay` and the error type of a missing file). The overlay belongs in core
because it is a property of the configuration document, which every host reads.

## Documentation Intent

- User guide: chapter 8 gains "Starting an application". Every example that starts a UI or a
  server in chapters 11 and 12 loads its configuration from YAML (AC-6). Store paths in a file
  resolve against the working directory, not the file's folder, which surprises anyone running
  `--config other/app.yaml`, so the guide says so. Phase 5 reviews every chapter `chapters-for`
  lists for `affects_docs`.
- `specs/guides/PYTHON_GUIDE.md` gains "Starting an application", and
  `specs/reference/PYTHON_INTEGRATION.md` the functions and their override rules.
- `liquers-py/python/liquers/__init__.pyi` gains `add_arguments`, `configure_from_args` and
  `configure_from_command_line`.
- API docs: the new Python module holding the argparse functions (`liquers/_cli.py`, or under
  `liquers.environment` if the submodule split lands first), and `liquers-py/src/config.rs` for
  the native override entry point.

## Design Dependencies

- `py-default-environment-config` (`configure`, `env.configure`) lands first or together, and
  `configure_from_args` is built on it.
- `py-package-submodules` places the new names in `liquers.environment`, and whichever lands
  second adds them.

## Open Questions

1. *(decided, Orest 2026-11-15)* **The command-line options:**
   - `--config`, `--command-registry` and a repeatable `--default-namespace` (as
     `liquers-validate` uses);
   - the server's `--host`, `--port`, `--prefix` and `--server-read-only`, a name that cannot be
     mistaken for a read-only store mount;
   - `--job-capacity`, `--async-loops`, `--pickle` / `--no-pickle` and `--conversion`.

   Every option defaults to "not given", so the file keeps its value. `options=[…]` limits the
   list.
2. *(decided, Orest 2026-11-15)* **The default file:** `liquers.yaml` in the working directory, as
   `liquers-agent` defaults to `liquers-agent.yaml`. `default_config=` changes it, and `None` turns
   it off.

## Findings carried to Phase 2

- Overrides cannot be applied in Python. A `dict` loses `ConfigDocument::base`, the sections of
  `ConfigDocument` are private, and the `Environment` keywords carry neither `server:` settings
  nor `command_registry`. They need a native entry point, for example a `configure(...)` keyword
  that takes overrides, applied to `PyConfig` after `PyConfig::from_document`. `--config` must
  reach Rust as a `pathlib.Path`, because a `str` is read as YAML text.
- `ConfigDocument::load` (`liquers-core/src/environment_config.rs`) reports a missing file as a
  general error, so AC-3 either checks for the file first or changes that error to
  `ConfigurationError`.
- `Environment::server_config` starts from the configured `server:` section and applies only the
  keywords given, so `lq.serve()` picks up an overridden port (AC-2).
- Boolean options need `default=None` (`argparse.BooleanOptionalAction`). A `store_true` would
  override the file on every run.
