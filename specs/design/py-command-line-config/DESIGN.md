---
id: PY-COMMAND-LINE-CONFIG
kind: design
title: Start a Python Liquers application from a configuration file with command-line overrides
form: compact
workflow: liquers-project
gh_pr: [273]
phase: architecture
area: [py, docs, core/context, core/error]
issues: [PY-APPLICATION-CANNOT-START-FROM-A-CONFIG-FILE]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/PYTHON_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, docs/guides/python/08-environment.md, docs/guides/python/11-ui.md, docs/guides/python/12-serving.md]
created: 2026-11-14
---
# Start a Python Liquers application from a configuration file with command-line overrides

Split from [`py-default-environment-config`](../py-default-environment-config/DESIGN.md) on
2026-11-14 to keep both within the size limits, and built on its `configure`. Phase 1 was written
and reviewed twice on 2026-11-14 (conformity and records, then codebase and soundness) while it
was still part of that design. Phase 1 approved 2026-11-15 (Orest: "proceed", accepting both
recommendations below, with a gate after each phase). Phase 2 written 2026-11-15.

## Phase 1: High-Level Design

### Purpose

Let a program start the usual way: add the Liquers options to its own `argparse` parser, pick the
configuration file (named, or a default), apply command-line overrides such as `--port` and
`--command-registry`, and configure the environment. It works step by step or in one call (Orest,
2026-11-14).

### Problem Example

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

### Scope and Acceptance Criteria

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

### Documentation Intent

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

### Design Dependencies

- `py-default-environment-config` (`configure`, `env.configure`) lands first or together, and
  `configure_from_args` is built on it.
- `py-package-submodules` places the new names in `liquers.environment`, and whichever lands
  second adds them.
- Crates: `liquers-py`, and `liquers-core` (`environment_config.rs`: `ConfigDocument::overlay` and
  the error type of a missing file).

### Open Questions

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

### Findings carried to Phase 2

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

## Phase 2: Architecture

### Solution

The command-line layer is pure Python (`liquers/_cli.py`). It turns the given options into an
*overlay*: a partial configuration document such as `{"server": {"port": 9000}}`. It then calls
`env.configure(config=<file>, overrides=<overlay>)`. The overlay is merged natively over the parsed
document, so the file's folder (`ConfigDocument::base`) still applies to the file's own
`command_registry:`. A command-line path is made absolute in Python first, so it resolves against
the working directory (AC-4). The merge rule: mappings merge key by key, and any other value,
lists included, replaces. The `Environment` keywords still win over both.

Rejected: merging a `dict` in Python, or editing the YAML text, both of which lose `base`; and a
keyword per server setting on `configure`, which would duplicate the document's schema.

Known-issue preflight:
- `STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE` (P3) is documented in chapter 8, not
  fixed here, and does not block.
- `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` (P2): the guide marks `--pickle` and the server
  options as sensitive, using the chapter 12 wording. It does not block.
- `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` (P3): `overlay` is host-independent, so it
  helps rather than adds to it.
- `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` (P2): the missing-file error is one instance of it,
  fixed here. It does not block.
- `USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED` (P2): the "runnable" chapter 8 examples of AC-6 are
  tested by Python tests that mirror them until that issue is fixed. It does not block.
- `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` (P3): `_cli.py` is written to §3. It does not
  block.

Nothing blocks. No command namespace is involved.

### Changes

`liquers-core/src/environment_config.rs` (`ConfigDocument`, which must be called before any
`take_section`):

```rust
/// Merges `overlay` (a partial document) over this one: mappings merge key by key, every other
/// value replaces, and `null` is an empty overlay. Refuses a non-mapping overlay (ParseError), a
/// section already taken, or environment keys that no longer parse (ConfigurationError). `base`
/// is kept, so a relative `command_registry` in the overlay resolves against the file's folder.
pub fn overlay(&mut self, overlay: serde_json::Value) -> Result<(), Error>;
```

To merge the environment's keys, the document keeps their raw JSON beside the parsed
`EnvironmentConfig` (a private `environment_raw` map) and re-parses after the merge, so nothing is
serialized back. A direct edit to the public `environment` field before `overlay` is therefore
replaced by the re-parse, which the method's doc says. `command_registry` is replaced as written:
`command_registry_path` skips `base` for an absolute path, and the Python layer always passes an
absolute one. `load` is unchanged, apart from reporting a missing file as
`Error::configuration_error` instead of `general_error`, so `lq.Environment(config=Path(…))` raises
`ConfigurationError` too. Only liquers-py calls `load` outside tests, and no code matches on the
old type. `liquers-agent` has its own loader and is not affected.

`liquers-py/src/config.rs`: `PyConfig::from_python(py, config, overrides: Option<&Bound<PyAny>>)`
converts `overrides` (a `dict`) to JSON with the existing `json.dumps` path, and calls
`doc.overlay(...)` before `PyConfig::from_document`. With `config=None` and `overrides` given, it
starts from `ConfigDocument::default()`, so `--port 9000` with no file still applies (AC-3).
`Settings::resolve` (from `py-default-environment-config`), `Environment(...)` and `configure(...)`
all gain `overrides=None`.

`liquers-py/python/liquers/_cli.py` (new; re-exported by `liquers/__init__.py`):

```python
def add_arguments(parser: argparse.ArgumentParser, *, options: Iterable[str] | None = None,
                  default_config: str | os.PathLike | None = "liquers.yaml") -> argparse._ArgumentGroup: ...
def configure_from_args(args: argparse.Namespace, *, env: Environment | None = None) -> None: ...
def configure_from_command_line(parser: argparse.ArgumentParser | None = None,
                                argv: Sequence[str] | None = None, *, env: Environment | None = None,
                                options: Iterable[str] | None = None,
                                default_config: str | os.PathLike | None = "liquers.yaml") -> argparse.Namespace: ...
```

Option to overlay mapping. Every option uses `default=None`, and its `dest` is prefixed with
`liquers_` so it cannot clash with the program's own destinations:

| Option | Overlay |
|---|---|
| `--config FILE` | the `config=` file (a `pathlib.Path`) |
| `--command-registry FILE` | `command_registry: <absolute path>` |
| `--default-namespace NS` (repeatable, `append`) | `default_namespaces: [...]` |
| `--host`, `--port`, `--prefix` | `server: {host / port / prefix}` |
| `--server-read-only` / `--no-server-read-only` (`BooleanOptionalAction`) | `server: {read_only}` |
| `--job-capacity N` | `assets: {job_capacity}` |
| `--async-loops N`, `--conversion {permissive,strict}`, `--pickle` / `--no-pickle` | `python: {…}` |

`add_arguments` records `default_config` with `parser.set_defaults(liquers_default_config=…)`.
`configure_from_args` takes `--config` if given and refuses a missing file. Otherwise it uses
`default_config` if that file exists, and otherwise no file. An option name the program already
uses raises argparse's own `ArgumentError` under the default `conflict_handler="error"` (AC-1).
Under `"resolve"` it silently replaces the program's option, which the docstring says. `options=`
takes option names without dashes, and `configure_from_args` reads each destination with
`getattr(args, dest, None)`, since a left-out option has none. `BooleanOptionalAction` is given no
`type`, `choices` or `metavar`, which are deprecated from 3.12.

Documents:
- User guide chapter 8, new section "8.x Starting an application": the step-by-step form and the
  single call, as runnable examples with a `liquers.yaml`.
- Chapters 11 and 12: their UI and server examples load a YAML file through
  `configure_from_command_line` or `configure`.
- `PYTHON_GUIDE.md`: "Starting an application". `PYTHON_INTEGRATION.md`: the overlay rule and the
  option table.
- API docs:
  - `_cli.py`, all five parts of `API_DOCUMENTATION_GUIDE.md` §3;
  - `liquers-core/src/environment_config.rs`, main components and an executed `overlay` example;
  - `liquers-py/src/config.rs`, main components.
- `specs/reference/ENVIRONMENT_CONFIG.md` §The configuration document: `overlay` and its merge
  rule; `load` reports a missing file as `ConfigurationError`.

### Risks

- Files: the three above, `__init__.py` / `__init__.pyi`, the guide chapters, and a
  `liquers-core` unit test for `overlay`.
- Compatibility: additive. The one behaviour change is that a missing configuration file
  raises `ConfigurationError` where it raised `GeneralError`. Only liquers-py sees it, and no code
  in the repository catches the old type.
- `argparse` differences across Python 3.10 to 3.15: `BooleanOptionalAction` exists since 3.9.
- Certainty: high. The overlay is a merge tested in core, under a thin Python layer.
