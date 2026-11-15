# Phase 4: Implementation Plan — py-package-submodules

## Overview

The work lands in one PR (#272, after the design) as four commits, with Steps 2 and 3 in one
commit. The order is: the native module moves; then the public modules and class names together
with the stubs; then the reference and module docs; then the documents. Each commit leaves the
suite green. There is no prerequisite issue (Phase 2 preflight).

Proof commands use `V=target/liquers-py-venv-cpython3.14` (the typing leg), which
`scripts/test-liquers-py.sh` creates. The script always runs the whole suite, so a single file
between runs is `$V/bin/python -m pytest liquers-py/tests/<file>` after
`maturin develop --manifest-path liquers-py/Cargo.toml --features debug-handles` in that venv.

## Progress

- [x] Step 1 (f9231d2): move the native module to `_liquers` behind `liquers/_backend.py`
- [x] Step 2 (0708cc8): public submodules, class module names, `liquers.error` (one commit with Step 3)
- [x] Step 3 (0708cc8): split the stubs per submodule (one commit with Step 2)
- [x] Step 4 (1a10c43): API reference per submodule and module docs
- [x] Step 5 (the commit after 1a10c43, with Phase 5): documents

## Implementation Steps

### Step 1: move the native module to `_liquers` behind `liquers/_backend.py`
- Files / symbols:
  - Build:
    - `liquers-py/pyproject.toml` `[tool.maturin]` and its comment.
    - `liquers-py/src/lib.rs` `_native`: only the `#[pymodule]` name in this step; Step 4 writes the doc.
    - `.gitignore:27-28`; delete the stale `liquers-py/python/liquers/_native*`.
    - `.github/workflows/liquers-py-release.yml:193` (the `.pyd` glob).
    - `.github/workflows/liquers-py-package.yml:18` (the path filter).
    - `scripts/test_check_wheel.py:35`.
  - Python:
    - New `liquers-py/python/_liquers/__init__.py` and `py.typed`.
    - `git mv liquers-py/python/liquers/_native.pyi liquers-py/python/_liquers/__init__.pyi`.
    - New `liquers/_backend.py` (no stub).
    - `liquers/_env.py:7` and `liquers/_commands.py:40`.
    - `liquers/__init__.py`, which imports from `_backend`.
  - Tests:
    - `tests/typing/stubtest-allowlist.txt`: the two `_native` entries.
    - `tests/test_STUBS_declarations.py`: the stubtest arguments, `test_native_names_are_declared`,
      `test_declaration_files_exist`, and `test_package_sources_pass_mypy`, which adds
      `-p _liquers`.
    - `tests/test_RECORDS_columns.py`.
    - `tests/package/test_PACKAGE_wheel.py`: `_liquers/` files.
    - New `tests/test_LAYOUT_submodules.py` with `test_LAYOUT06_native_package_is_private`.
- Change:
  - `module-name = "_liquers"` and `python-packages = ["liquers"]`.
  - `#[pymodule(name = "_liquers", gil_used = false)]`; the Rust function keeps its name.
  - `_liquers/__init__.py` is a module doc, `from ._liquers import *`, and
    `__all__ = _liquers.__all__`.
  - Class modules stay `"liquers"` in this step.
- Depends on: none.
- Proof:
  1. `PYTHON=python3.14 bash scripts/test-liquers-py.sh` passes. It runs the whole suite, STUBS
     and LAYOUT06 included.
  2. `python3 -m pytest scripts/test_check_wheel.py` passes.
  3. `maturin build -m liquers-py/Cargo.toml -o target/wheel-check` builds a wheel. In a fresh venv
     with the wheel installed, `LIQUERS_PY_WHEEL_TEST=1 python -m pytest liquers-py/tests/package`
     passes. That proves R1 in a real wheel, which `maturin develop` alone cannot.
- Rollback: revert the commit.

### Step 2: public submodules, class module names, `liquers.error`
- Files / symbols:
  - New `liquers/{query, commands, metadata, dependencies, plan, recipes, store, environment,
    assets, error, data_nodes, pipeline, template, records, server, lui}.py`.
  - `liquers/__init__.py`, rewritten. It imports `liquers.error` first, then the submodules, then
    their names. `__all__` keeps its content, and `_ERROR_SUBCLASSES` becomes an alias.
  - `liquers/error.py`: the `__all__` holds `Error`, `ErrorType`, `LiquersError` and the generated
    classes.
  - `liquers/_errors.py`: the `__module__` values at lines 83, 111 and 126.
  - The 75 `#[pyclass(…, module = "liquers")]` attributes in `liquers-py/src/`, set from the Phase 1
    map. `QuerySegment` (`src/query.rs:429`) gets `liquers.query`.
  - `tests/test_LAYOUT_submodules.py` gains LAYOUT01 to LAYOUT05.
  - `tests/test_OBJECT_conformance.py`: OBJECT08.
- Change: as in Phase 2 *Integration Points* (Errors, Public modules). Each new file gets a
  first-version module doc; Step 4 completes it.
- Depends on: Step 1.
- Proof: `$V/bin/python -m pytest liquers-py/tests --deselect
  liquers-py/tests/test_STUBS_declarations.py` passes. The stubs still place every class in
  `__init__.pyi` until Step 3, so stubtest is expected to fail here, and Step 3's proof covers
  both steps.
- Rollback: revert the shared commit of Steps 2 and 3.

### Step 3: split the stubs per submodule
- Files / symbols:
  - `liquers/__init__.pyi` is split into `liquers/<sub>.pyi`. The stub-only helpers go where
    Phase 2 places them, and the error declarations (`__init__.pyi:1670` onwards) go to
    `error.pyi`.
  - `tests/typing/stubtest-allowlist.txt`: `liquers.environment.Environment.live_handle_count`.
  - `tests/test_STUBS_declarations.py`: the `remaining()` regex, `test_stubtest_reports_drift`,
    `stub_classes` and `test_error_classes_are_declared`.
  - New `tests/typing/good/submodules.py`.
  - `tests/package/test_PACKAGE_wheel.py`: the submodule stubs.
- Change: each declaration moves with its name, and the stubs import from one another as needed.
  `__init__.pyi` binds the submodules and re-exports every name.
- Depends on: Step 2.
- Proof: `PYTHON=python3.14 bash scripts/test-liquers-py.sh` passes, which is the whole suite with
  STUBS. Then commit Steps 2 and 3 together.
- Rollback: revert the shared commit.

### Step 4: API reference per submodule and module docs
- Files / symbols:
  - `tests/test_DOCS_api_reference.py`: `test_DOCS04_api_reference_has_a_page_per_submodule`.
  - `tests/test_LAYOUT_submodules.py`: LAYOUT07 and LAYOUT08.
  - `scripts/test-liquers-py.sh:73-74`: doctests over `python/liquers/*.py` and
    `python/_liquers/__init__.py`.
  - `scripts/check_api_docs.py` `python_docs`: also collects `python/_liquers/`.
  - Module docs of the sixteen submodules, `_backend.py`, `_liquers/__init__.py`,
    `liquers/__init__.py` (the module map, and "never `_liquers`"), and `liquers-py/src/lib.rs`
    (the `//!` block and the line-76 comment).
- Change: each module doc follows `API_DOCUMENTATION_GUIDE.md` §3, names its Rust counterpart, and
  has a doctest. `_backend.py` covers all five parts, although the checker requires only its
  purpose.
- Depends on: Step 3.
- Proof:
  - `python3 scripts/check_api_docs.py --strict` on each touched file exits 0 with no output.
  - `python3 -m pytest scripts/` passes, covering `check_api_docs`' own tests.
  - `PYTHON=python3.14 bash scripts/test-liquers-py.sh` passes, which covers the doctests and pdoc.
- Rollback: revert the commit.

### Step 5: documents
- Files / symbols:
  - The Phase 2 *Documentation Architecture* table.
  - The user-guide chapters that `python3 scripts/docs_index.py chapters-for <affects_docs>`
    lists, which are reviewed and updated where they name the layout.
  - The open issues that name the old layout: `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED`
    (`_native.pyi`, and its class now reports `liquers.query`) and `PY-ABI3T-WHEEL-NOT-USED`
    (the library path).
- Change: as in the table. Every `reference/` and `guides/` document gets a `## History` row and a
  `reviewed:` bump (§9.2), and each user-guide chapter gets a `HISTORY.md` row.
- Depends on: Step 4.
- Proof:
  - `python3 scripts/docs_index.py --check` reports 0 errors.
  - `cargo run -p liquers-docs -- check` passes.
  - `$V/bin/python -m pytest liquers-py/tests/test_GUIDE_examples.py` passes.
  - `grep -rn "liquers\._native\|_native\.pyi" --exclude-dir=target --exclude-dir=archive
    --exclude-dir=design .` finds nothing.
- Rollback: revert the commit.

## Testing Plan

During a step, run its proof. After Step 5:
- run `PYTHON=python3.14 bash scripts/test-liquers-py.sh` and `PYTHON=python3.13 bash
  scripts/test-liquers-py.sh` once each;
- repeat the wheel check of Step 1 (`maturin build` and the `tests/package` suite in a fresh venv).

No `cfg(feature)` or optional dependency changes, so the build matrix is not needed. CI on the PR
runs `liquers-py-tests.yml` on 3.10, 3.14 and 3.14t, and `liquers-py-package.yml`.

## Rollback Plan

Revert the series. Each commit can be reverted alone, and newest first is always safe. If only
Step 1 lands, the package works with the flat API over `_liquers`.

## Documentation Updates

The documents of Step 5 and the module docs of Step 4 are checked with `scripts/check_api_docs.py
--strict` (`API_DOCUMENTATION_GUIDE.md` §10). Regenerated files are `specs/index.csv`,
`specs/index.md` and the `specs/README.md` blocks, through `scripts/docs_index.py`.
`specs/command_registry.yaml` stays untouched, because no command changes. Each changed
`reference/` or `guides/` document gets a History row and a `reviewed:` bump.

## Phase 5 Entry Criteria
- [x] Implementation finished and validated
- [x] User and review comments answered
- [x] Documentation checkable against implemented and tested behaviour
