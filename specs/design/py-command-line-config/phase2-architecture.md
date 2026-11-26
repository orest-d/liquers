# Phase 2: Solution & Architecture — py-command-line-config

## Overview

The command-line layer is pure Python (`liquers/_cli.py`). It turns the given options into an
*overlay*: a partial configuration document such as `{"server": {"port": 9000}}`. It then calls
`env.configure(config=<file>, overrides=<overlay>)`. The overlay is merged natively over the parsed
document, so the file's folder (`ConfigDocument::base`) still applies to the file's own
`command_registry:`. A command-line path is made absolute in Python first, so it resolves against
the working directory (AC-4). The merge rule: mappings merge key by key, and any other value,
lists included, replaces. The `Environment` keywords still win over both.

Rejected: merging a `dict` in Python, or editing the YAML text, both of which lose `base`; and a
keyword per server setting on `configure`, which would duplicate the document's schema.

## Known-Issue Preflight

- `STORE-PATHS-ARE-NOT-RELATIVE-TO-THE-CONFIGURATION-FILE` (P3) is documented in chapter 8, not
  fixed here, and does not block.
- `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` (P2): the guide marks `--pickle` and the server
  options as sensitive, using the chapter 12 wording. It does not block.
- `HOSTS-READ-CONFIGURATION-DOCUMENTS-DIFFERENTLY` (P3): `overlay` is host-independent, so it
  helps rather than adds to it.
- `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` (P2): the missing-file error is one instance of it,
  fixed here. It does not block.
- `USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED` (P2): the "runnable" chapter 8 examples of AC-6 are
  run by `liquers-py/tests/test_GUIDE_examples.py`, which already executes the Python guide's
  blocks; the issue is stale for the Python guide. It does not block.
- `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` (P3): `_cli.py` is written to §3. It does not
  block.

Nothing blocks. No command namespace is involved.

## Interfaces

`liquers-core/src/environment_config.rs` (`ConfigDocument`, which must be called before any
`take_section`):

```rust
/// Merges `overlay` (a partial document) over this one: mappings merge key by key, every other
/// value replaces, and `null` is an empty overlay. Refuses a non-mapping overlay or environment keys
/// that no longer parse (ParseError, as `from_json`), and a section already taken
/// (ConfigurationError). `base`
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

## Integration Points

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

## Error Handling

| Case | Error |
|---|---|
| `--config` names a missing file (AC-3) | `Error::configuration_error` naming it: from `ConfigDocument::load`, and checked first in `_cli.py` |
| A non-mapping overlay | `Error::parse_error` |
| An overlay over a section already taken, or environment keys that no longer parse | `Error::configuration_error` |
| A bad option value (`--port abc`, `--async-loops 0`) | argparse's own usage error for the type, and `configure`'s existing errors for the range |
| A clashing option name (AC-1) | `argparse.ArgumentError`, raised by argparse |

## Relevant Commands

None.

## Documentation Architecture

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

## Risks

- Files: the three above, `__init__.py` / `__init__.pyi`, the guide chapters, and a
  `liquers-core` unit test for `overlay`.
- Compatibility: additive. The one behaviour change is that a missing configuration file
  raises `ConfigurationError` where it raised `GeneralError`. Only liquers-py sees it, and no code
  in the repository catches the old type.
- `argparse` differences across Python 3.10 to 3.15: `BooleanOptionalAction` exists since 3.9.
- Certainty: high. The overlay is a merge tested in core, under a thin Python layer.
