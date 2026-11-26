# Phase 5: Documentation - py-command-line-config

## Completion Preconditions

- [x] Implementation is finished and validated: `test_CLI_startup.py` (CLI01 to CLI06) and the
  guide examples of chapters 8, 11 and 12 pass; `cargo test -p liquers-core --lib
  environment_config` 23 passed
- [x] All user comments are answered or incorporated (the option set and default file accepted
  with "proceed", 2026-11-15; "proceed all")
- [x] All review comments are answered or incorporated (two reviews per phase, applied)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR (#273)

## Implementation Summary

A Liquers application can start from a configuration file with command-line overrides, step by
step or in one call, as Orest asked on 2026-11-14:

- `lq.add_arguments(parser)` adds a "Liquers" group to the program's own `argparse` parser:
  `--config`, `--command-registry`, `--default-namespace`, `--host`, `--port`, `--prefix`,
  `--server-read-only`, `--job-capacity`, `--async-loops`, `--conversion` and `--pickle`. Every
  option defaults to "not given", so the file keeps its value (AC-1, AC-2).
- `lq.configure_from_args(args)` configures from `--config`, or from `liquers.yaml` in the working
  directory if it exists, with the given options merged over it (AC-2, AC-3).
- `lq.configure_from_command_line(parser)` does all three steps and returns the parsed arguments
  (AC-5).

All three take `env=`. The merge is native: `ConfigDocument::overlay` (liquers-core), reached
through `overrides=` on `Environment`, `configure` and `lq.configure`. That keeps the file's
folder, so a relative `command_registry` in the file still resolves against it, while a relative
`--command-registry` resolves against the working directory (AC-4). `ConfigDocument::load` now
reports a missing file as `ConfigurationError`.

The user guide's server and UI examples load a YAML configuration (AC-6), as Orest asked for
Phase 5. Nothing was omitted. Added beyond the design: `options=` rejects an unknown name with
`ValueError`.

## Documentation Delivered

### New Reference Documents
None. `overlay` belongs in `reference/ENVIRONMENT_CONFIG.md` §The configuration document, and
`overrides=` in `reference/PYTHON_INTEGRATION.md` §3.

### New Guide Documents
None. Usage went into `guides/PYTHON_GUIDE.md` §4 "Starting an application" and the user guide's
§8.7.

### Existing Documents Reviewed or Updated
`affects_docs`:
- `specs/reference/ENVIRONMENT_CONFIG.md`: Overlay, and the missing-file error.
- `specs/reference/PYTHON_INTEGRATION.md`: Overrides.
- `specs/guides/PYTHON_GUIDE.md`: §4 Starting an application.
- Chapters 08, 11 and 12.

Each has a History row and `reviewed: 2026-11-15`.

### Module Docs
- `liquers-py/python/liquers/_cli.py`: new, written to §3, with an executed example.
- `liquers-core/src/environment_config.rs`: `overlay` in Main components, with a doc test.
- `liquers-py/src/config.rs`: `overrides=`.

`check_api_docs.py`: 0 warnings, 0 errors.

### User-Guide Chapters
These are the same 19 chapters as `py-default-environment-config` (see its Phase 5).
- Chapter 8: new §8.7 "Starting an application", with the option table, a step-by-step example,
  a one-call example and `overrides`.
- Chapter 11: every example starts from a `liquers.yaml`, and the window-with-server example uses
  its `server` section.
- Chapter 12: the service, server and settings examples load YAML, plus a server configured from
  its command line.
- Rust 06, 08 and 12, and Web 08: reviewed for `overlay` and the missing-file error, still right.

### Links and Capability Map
The new `specs/README.md` entry is shared with `py-default-environment-config`.

## Issues Filed

None new. `USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED` gained an update: it is stale for the Python
guide, whose examples `test_GUIDE_examples.py` runs, and stays open for Rust and the web.
`PY-APPLICATION-CANNOT-START-FROM-A-CONFIG-FILE` is closed.

## Important Learning

- A `dict` passed as `config` has no folder, so overrides must be merged into the parsed document,
  not into the text. That is why `overlay` lives in core and is reached through `overrides=`.
- Boolean options need `BooleanOptionalAction` with `default=None`. A `store_true` would override
  the file on every run.

## Conformance and Remaining Work

Requested: a convenient argparse start-up, step by step and in one call, with examples, and
YAML-configured server and UI examples. Approved: AC-1 to AC-6. Implemented: all six, each
tested by its `test_CLI0n` test and the core `overlay_*` tests. Nothing remains.

## Validation

- `pytest tests/test_CLI_startup.py tests/test_GUIDE_examples.py`.
- `cargo test -p liquers-core --lib environment_config`, plus its doc test.
- `check_api_docs.py`.
- `cargo run -p liquers-docs -- check`.
- `docs_index.py --check`.
