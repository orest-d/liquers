# Phase 4: Implementation Plan — liquers-py-package

## Overview

This design lands first in M5, in the shared branch, because `liquers-py-records` uses
`_extras.py` and `liquers-py-stubs` declares `__version__`, `require` and `liquers_version`.
The order runs from what everything else reads to what only CI runs:
- the manifests (version and license);
- the version accessors;
- `require`;
- the package metadata;
- the wheel checker;
- the wheel tests;
- the two workflows.

No issue has to be fixed first: the Phase 2 preflight's blockers are this milestone's own
designs, and the OpenSSL issue is closed. Steps 3 and 5 can run in parallel with Step 2.

## Progress

- [ ] Step 1: one workspace version and license, and their checker
- [ ] Step 2: `liquers_version` in core, Python and JavaScript
- [ ] Step 3: `lq.require` and the extras table
- [ ] Step 4: package metadata, the `wheel` profile, notices and the quick-start
- [ ] Step 5: the wheel and `dist/` checker
- [ ] Step 6: the installed-wheel tests
- [ ] Step 7: the release and PR workflows, and their first run

## Implementation Steps

### Step 1: one workspace version and license, and their checker

- Files / symbols:
  - root `Cargo.toml` `[workspace.package]`;
  - every root member's `Cargo.toml`;
  - `liquers-template/Cargo.toml` (`exclude = ["tests/spec"]`) and its `src/mustache/encode.rs`
    test loader;
  - `liquers-py/LICENSE` (a copy);
  - new `scripts/check-workspace-metadata.py` and `scripts/test_check_workspace_metadata.py`.
- Change: as Phase 2. The script takes `[--root DIR] [--tag TAG]`, reads TOML with `tomllib`, and
  exits 1 with one line per finding.
- Depends on: none.
- Proof:
  - `python3 -m pytest scripts/test_check_workspace_metadata.py`;
  - `cargo package -p liquers-template --list --allow-dirty | grep -c tests/spec` prints 0;
  - `cargo test -p liquers-template`;
  - `cargo metadata --format-version 1 --no-deps` shows every package at `0.1.0` with
    `AGPL-3.0-or-later`.
- Rollback: revert; versions return to per-crate.

### Step 2: `liquers_version` in core, Python and JavaScript

- Files / symbols:
  - `liquers-core/src/context.rs` `Environment::liquers_version`;
  - `liquers-py/src/lib.rs` `pep440` and the module's `__version__`;
  - `liquers-py/src/environment.rs` the `liquers_version` getter;
  - `liquers-py/python/liquers/__init__.py`;
  - `liquers-web/src/environment.rs` `liquersVersion`;
  - `liquers-web/tests/e2e/package.spec.ts` (version reading).
- Change: Phase 2's signatures. The Python getter returns `liquers_core::VERSION` without touching
  the environment cell.
- Depends on: Step 1, for the version the tests compare.
- Proof:
  - `cargo test -p liquers-core --lib liquers_version`;
  - `cargo test -p liquers-py --no-default-features --features debug-handles --lib`
    (`pep440_rewrites_prereleases`, `liquers_version_is_core_version`);
  - `bash scripts/test-liquers-py.sh -k ENVIRON` (the two `test_ENVIRON_liquers_version_*`);
  - `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test
    environment_ENVIRON`, after `cargo clean`.
- Rollback: revert; all additive.

### Step 3: `lq.require` and the extras table (parallel with Step 2)

- Files / symbols: new `liquers-py/python/liquers/_extras.py` (`EXTRAS`, `require`); and
  `__init__.py`, which exports `require`.
- Change: Phase 2's `require`. `EXTRAS` maps each extra to its modules, pip package names and
  the platforms without a wheel, as one dict.
- Depends on: none.
- Proof: `bash scripts/test-liquers-py.sh -k require`, with new unit tests in
  `tests/test_PACKAGE_require.py`:
  - a missing module's exact text;
  - "not available" under a monkeypatched `sys.platform` / `platform.machine()` /
    `Py_GIL_DISABLED`;
  - the unknown extra.
- Rollback: revert; nothing imports it yet.

### Step 4: package metadata, the `wheel` profile, notices and the quick-start

- Files / symbols:
  - `liquers-py/pyproject.toml`;
  - root `Cargo.toml` `[profile.wheel]`;
  - `liquers-py/Cargo.toml` `abi3` feature;
  - new `liquers-py/about.toml` and `about.hbs`;
  - `.gitignore` (`THIRD-PARTY-NOTICES.html`);
  - `liquers-py/README.md` (install, the quick-start markers, the platform list).
- Change: as Phase 2. The README states the license and which platforms are source-only.
- Depends on: Steps 1 and 3 (the extras).
- Proof:
  - `maturin build --profile wheel --features abi3 -o /tmp/w` builds a `cp314-abi3` wheel whose
    `METADATA` has `License-Expression` and five `Provides-Extra`;
  - `cargo about generate -m liquers-py/Cargo.toml about.hbs` succeeds, which proves every
    dependency license is accepted;
  - `bash scripts/test-liquers-py.sh` still passes.
- Rollback: revert; development builds do not read `[profile.wheel]`.

### Step 5: the wheel and `dist/` checker (parallel with Step 2)

- Files / symbols: new `scripts/check-wheel.py`, `scripts/test_check_wheel.py` and
  `liquers-py/packaging/allowed-libraries.toml`.
- Change: Phase 2's two modes. `<wheel>` checks one wheel: grafted libraries, the allow-list and
  the size. `--dist DIR` checks the tag set and the sdist's `Cargo.lock`. The platform tool's
  output is read from a file or from stdin, so the tests feed it recorded output.
- Depends on: none.
- Proof: `python3 -m pytest scripts/test_check_wheel.py` (four tests), and `check-wheel.py` on
  Step 4's wheel with `auditwheel show` output passes.
- Rollback: revert.

### Step 6: the installed-wheel tests

- Files / symbols: new `liquers-py/tests/package/{conftest.py,test_PACKAGE_wheel.py}` and
  `tests/package/data/small.parquet`, written by a Rust test in
  `liquers-lib/tests/records_parquet_polars.rs` and checked as current, as the feather fixture is.
- Change: the Phase 3 test plan.
- Depends on: Steps 2-4.
- Proof: on Step 4's wheel, in a fresh venv, `LIQUERS_PY_WHEEL_TEST=1 RELEASE_VERSION=0.1.0
  pytest --rootdir liquers-py/tests/package -p no:cacheprovider liquers-py/tests/package`
  passes. `test_extras` runs on Linux x86_64 here; its other cells run in Step 7.
- Rollback: revert; the module skips outside the `test` jobs.

### Step 7: the release and PR workflows, and their first run

- Files / symbols: new `.github/workflows/liquers-py-release.yml` and `liquers-py-package.yml`.
- Change: Phase 2's jobs and triggers. Publishing jobs carry `if: startsWith(github.ref,
  'refs/tags/liquers-py-v')`.
- Depends on: Steps 1-6.
- Proof:
  - the PR's own runs: `liquers-py-package` green;
  - `liquers-py-release` green on its pull-request trigger, across all fifteen cells, with the
    `dist` step passing and no publishing job run.

  A cell that cannot be made to build (Windows arm64's `clang-cl`, the manylinux 3.15t
  interpreter) is a discovery against AC-1. It stops the work and goes to Orest, because it
  changes the wheel set he approved.
- Rollback: delete the workflows; nothing has been published.

## Testing Plan

- Each step's proof, as it lands.
- After Step 4: `bash scripts/test-liquers-py.sh` on 3.14 and 3.14t.
- Before pushing Step 7:
  - `cargo test -p liquers-lib --lib --tests`, since every manifest changed;
  - `bash scripts/check-build-matrix.sh`, because `liquers-py` gains a feature.
- In CI: the existing `liquers-py-tests`, `liquers-web-tests`, `build-matrix` and `docs-check`
  workflows, plus the two new ones.

## Rollback Plan

Each step reverts on its own. Steps 1 and 2 are what `liquers-py-records` and `liquers-py-stubs`
build on: reverting them later means reverting the stubs' declarations of `__version__` and
`liquers_version`. Nothing is published before a tag and Orest's approval, so no rollback ever
has to touch PyPI.

## Documentation Updates

Phase 2's Documentation Architecture, written in Phase 5:
- `PYTHON_INTEGRATION.md` §Packaging;
- `PYTHON_GUIDE.md` §1;
- `PROJECT_OVERVIEW.md`;
- `JAVASCRIPT_INTEGRATION.md`;
- `LANGUAGE-INTEGRATION_GUIDE.md` Appendix A and §6;
- `liquers-py/README.md`;
- `specs/README.md`.

Each gets a History row and a `reviewed:` bump. Also: `specs/index.csv` regenerated, and
`specs/command_registry.yaml` untouched (no command changes). The issue `PY-NO-PUBLISHED-WHEELS`
closes when the first PyPI release exists. Until then Phase 5 marks it `in_progress`, with the
release left to Orest.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
