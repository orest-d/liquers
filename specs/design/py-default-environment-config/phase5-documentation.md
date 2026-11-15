# Phase 5: Documentation - py-default-environment-config

## Completion Preconditions

- [x] Implementation is finished and validated: Python suite 673 passed, 34 skipped, after which
  only the two text checks of `py-command-line-config` failed and were fixed (re-run: passed);
  `cargo test -p liquers-py --no-default-features --features debug-handles --lib` 52 passed
- [x] All user comments are answered or incorporated (Orest's answers at the Phase 1 gate and
  "proceed all" at the Phase 2 gate, 2026-11-15)
- [x] All review comments are answered or incorporated (two reviews per phase, applied)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR (#273)

## Implementation Summary

The settings of a Python environment (`config`, `conversion`, `pickle`, `async_loops`,
`job_capacity` and the `command_registry` file) are no longer fixed when the environment is made.
They live in one `Settings` value under the declarations lock, and a build reads them there.
`env.configure(...)` and `lq.configure(...)` replace them until the first successful build, so the
default environment can be configured after the modules that register commands were imported,
in any order. This is what the issue asked for, in the form Orest chose (configure-before-
registration was rejected as fragile; no environment variable).

As approved:

- **Conversion bound per build.** `PyCallable` no longer holds conversion and pickle;
  `register_py_command` binds them (`BoundCallable`), so a command registered before `configure`
  converts under the configured mode (AC-2).
- **Refused once built (open question 1).** A build that succeeded fixes the settings;
  `configure` then raises `ConfigurationError` ("already built") and the environment works on
  (AC-4). A failed first build leaves `configure` allowed (AC-9).
- **Last configure wins (open question 2)** (AC-5), and a failing `configure` changes nothing
  (AC-6): it resolves, re-checks the declared template and pipeline commands (open question 3),
  and resizes the loop pool (`LazyLoopPool::resize`) before it assigns.
- **Registry file, then `import_metadata`** (AC-3). The registry file has its own slot in
  `Settings` and applies before the imported metadata. A missing registry file is now a
  `ConfigurationError`.
- `serve`, `ui`, `store_set` and `render` read the configured settings (AC-8).

Added beyond Phase 2: `overrides=` on `Environment`, `configure` and `lq.configure`, needed by
`py-command-line-config` and designed there.

## Documentation Delivered

### New Reference Documents
None. The behaviour is part of the Python environment's contract, so it went into
`reference/PYTHON_INTEGRATION.md` §3, as Phase 2 planned.

### New Guide Documents
None. Usage went into `guides/PYTHON_GUIDE.md` §4, as Phase 2 planned.

### Existing Documents Reviewed or Updated
`affects_docs`:
- `specs/reference/PYTHON_INTEGRATION.md`: §3 Construction (`overrides=`), Settings, Configure,
  the registry file, the build order, Rebuild, Default environment, the methods table.
- `specs/guides/PYTHON_GUIDE.md`: §4 "Configuring the default environment".
- Chapters 02, 08 and 12 (below).

Each has a History row and `reviewed: 2026-11-15`.

### Module Docs
- `liquers-py/src/environment.rs`: retained declarations and `Settings`, configure, main
  components, example.
- `liquers-py/python/liquers/_env.py`: full module docstring.
- `liquers-py/src/config.rs`: `overrides=`, where it is used.

`python3 scripts/check_api_docs.py` reports 0 warnings and 0 errors for these files and for
`aio.rs`, `command.rs`, `value.rs` and `context.rs`.

### User-Guide Chapters
`docs_index.py chapters-for` lists 19 chapters across the three guides for this design and
`py-command-line-config` together.
- Fixed: Python 01 (one sentence), 02 (the default-environment note), 08 (§8.1 `lq.configure`,
  §8.2 `env.configure`, §8.3–8.4 when files and settings are read; §8.7 belongs to
  `py-command-line-config`), 11 and 12 (YAML-configured examples, also
  `py-command-line-config`).
- Reviewed, unchanged: the other Python chapters, Rust 06, 08 and 12, and Web 08.

Every row is in that guide's `HISTORY.md`. The Python chapters' examples run under
`test_GUIDE_examples.py`.

### Links and Capability Map
`specs/README.md` has a new entry, "Configuring the Python default environment and starting an
application from a configuration file". It points to the reference and the guide, with both
designs.

## Issues Filed

None new. `PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED` is closed.

## Important Learning

- The default environment is process-wide, so every test of `lq.configure` needs a fresh
  interpreter (`run_script` in `test_CONFIGURE_environment.py`).
- "Built" means a store exists (`Declarations.store`). Only a successful build sets it, which is
  why a failed build still allows `configure`.
- The Rust unit tests of liquers-py link only in `scripts/test-liquers-py.sh`'s configuration
  (`--no-default-features --features debug-handles`), not under a plain `cargo test -p
  liquers-py`. The Phase 4 proofs were corrected to say so.

## Conformance and Remaining Work

Requested: configure the default environment, documented, independent of import order. Approved:
AC-1 to AC-9. Implemented: all nine, each tested by its `test_CONFIGURE0n` test, plus the two
Rust tests. Nothing remains. Reconfiguring a built environment is out of scope (open question 1)
and is not filed: nobody has asked for it.

## Validation

- `bash scripts/test-liquers-py.sh` (Python suite; Rust unit tests).
- `pytest tests/test_CONFIGURE_environment.py tests/test_CLI_startup.py tests/test_GUIDE_examples.py`.
- `python3 scripts/check_api_docs.py` on the touched modules.
- `cargo run -p liquers-docs -- check`.
- `python3 scripts/docs_index.py --check`.
