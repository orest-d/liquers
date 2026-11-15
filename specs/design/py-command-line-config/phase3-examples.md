# Phase 3: Examples & Use-cases — py-command-line-config

## Overview Table

| # | Kind | Name | Scenarios |
|---|---|---|---|
| 1 | Example | Step by step | AC-1, AC-2 |
| 2 | Example | One call | AC-5 |
| 3 | Example | Chapters 11 and 12 | AC-6 |
| 4 | Test | `environment_config::tests::overlay_merges_mappings_and_replaces_other_values` | AC-2 |
| 5 | Test | `environment_config::tests::overlay_keeps_base_and_reparses_environment_keys` | AC-4 |
| 6 | Test | `environment_config::tests::overlay_refuses_a_taken_section_and_a_bad_environment` | AC-2 |
| 7 | Test | `environment_config::tests::load_reports_a_missing_file_as_a_configuration_error` | AC-3 |
| 8 | Test | `test_CLI01_add_arguments_adds_a_group` | AC-1 |
| 9 | Test | `test_CLI02_configure_from_args_overrides_the_file` | AC-2 |
| 10 | Test | `test_CLI03_default_file_and_missing_file` | AC-3 |
| 11 | Test | `test_CLI04_paths_keep_their_meaning` | AC-4 |
| 12 | Test | `test_CLI05_configure_from_command_line` | AC-5 |
| 13 | Test | `test_CLI06_guide_examples_load_yaml` | AC-6 |

## Examples

**Example 1: step by step** (user guide chapter 8, "Starting an application"; run by
`test_GUIDE_examples.py` with `argv` given, so it needs no real command line):

```python
import argparse, pathlib, tempfile
import liquers as lq

folder = pathlib.Path(tempfile.mkdtemp())
(folder / "app.yaml").write_text("server: {host: 127.0.0.1, port: 8080}\n")

parser = argparse.ArgumentParser(description="My Liquers app")
parser.add_argument("--title", default="My app")          # the program's own option
lq.add_arguments(parser)
args = parser.parse_args(["--config", str(folder / "app.yaml"), "--port", "0"])
lq.configure_from_args(args)
server = lq.serve()                                       # port 0 from the command line wins
assert server.port != 8080 and args.title == "My app"
server.shutdown()
```

**Example 2: one call**, which adds only the delta:

```python
args = lq.configure_from_command_line(parser, argv=["--config", str(folder / "app.yaml")])
```

**Example 3: chapters 11 and 12.** Each server or UI example writes a small `liquers.yaml` and
calls `lq.configure(config=…)` or `lq.configure_from_command_line(argv=[…])` before `lq.serve` or
`lq.ui` (AC-6).

## Edge and Error Cases

- An option not given must not override the file. Booleans and `append` default to `None`
  (CLI02).
- The program defines `--port` itself: `ArgumentError`, or `options=` leaves it out (CLI01).
- A configuration given as YAML text has no folder, so its relative `command_registry`
  resolves against the working directory, as today (CLI04 notes it).
- `configure_from_args` after the default is built raises `configure`'s "already built"
  error, which is covered by `py-default-environment-config` AC-4.

## Test Plan

`liquers-core/src/environment_config.rs` tests:
- `overlay_merges_mappings_and_replaces_other_values` (AC-2): `server.port` replaced,
  `server.host` kept, a list replaced, `null` a no-op.
- `overlay_keeps_base_and_reparses_environment_keys` (AC-4): a relative `command_registry` in the
  file still joins `base`, an absolute overlay path is kept, and `default_namespaces` comes from
  the overlay.
- `overlay_refuses_a_taken_section_and_a_bad_environment` (AC-2): `ConfigurationError` in both
  cases.
- `load_reports_a_missing_file_as_a_configuration_error` (AC-3).

`liquers-py/tests/test_CLI_startup.py` (each test that touches the default environment runs in a
subprocess):
- `test_CLI01_add_arguments_adds_a_group` (AC-1): the group and its options are in `--help`, and
  the program's own options are untouched. A clashing `--port` raises `argparse.ArgumentError`,
  and `options=["config"]` adds only `--config`.
- `test_CLI02_configure_from_args_overrides_the_file` (AC-2): `--port 0 --command-registry
  cmr.yaml --server-read-only` over a file with port 8080 and `read_only: false`. The server binds
  a free port, refuses a store write, and the registry's label is visible. A run with no options
  keeps every file value, booleans included. `env=` configures that environment.
- `test_CLI03_default_file_and_missing_file` (AC-3), parametrized:
  - `liquers.yaml` present in the working directory is used;
  - absent, the defaults are used;
  - `default_config=None` reads nothing;
  - a missing `--config` raises `ConfigurationError`;
  - `--port 9000` with no file applies to the defaults.
- `test_CLI04_paths_keep_their_meaning` (AC-4): the file's relative `command_registry` resolves
  against the file's folder, and a relative `--command-registry` against the working directory.
- `test_CLI05_configure_from_command_line` (AC-5): `argv=` is parsed and the namespace returned,
  with the program's options. With `argv=None`, it reads `sys.argv[1:]` (set in the subprocess).
- `test_CLI06_guide_examples_load_yaml` (AC-6): every block in chapters 11 and 12 that calls
  `serve` or `ui` also calls `configure`. A text check, beside `test_GUIDE_examples.py`, which
  runs them.

Run: `cargo test -p liquers-core --lib environment_config`; `pytest tests/test_CLI_startup.py`.
