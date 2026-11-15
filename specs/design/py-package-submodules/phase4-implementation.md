# Phase 4: Implementation Plan — py-package-submodules

## Overview

The work lands in one PR (#272, after the design) as four commits (Steps 2 and 3 together). The order is: the native module
moves; then the public modules and class names; then the stubs; then the reference and module
docs; then the documents. Each step leaves the suite green, so a step's proof is the tests it
touches plus the parts of the suite that could break. There is no prerequisite issue (Phase 2
preflight).

## Progress

- [ ] Step 1: move the native module to `_liquers` behind `liquers/_backend.py`
- [ ] Step 2: public submodules, class module names, `liquers.error`
- [ ] Step 3: split the stubs per submodule
- [ ] Step 4: API reference per submodule and module docs
- [ ] Step 5: documents

## Implementation Steps

### Step 1: move the native module to `_liquers` behind `liquers/_backend.py`
- Files / symbols:
  - `liquers-py/pyproject.toml` `[tool.maturin]`
  - `liquers-py/src/lib.rs` `_native` (`#[pymodule]`, the `//!` text and the line-76 comment)
  - new `liquers-py/python/_liquers/__init__.py` and `py.typed`
  - `git mv liquers-py/python/liquers/_native.pyi liquers-py/python/_liquers/__init__.pyi`
  - new `liquers/_backend.py` and `_backend.pyi`
  - `liquers/_env.py:7`, `liquers/_commands.py:40`
  - `liquers/__init__.py`, which imports from `liquers._backend`
  - `.gitignore:27-28`
  - `.github/workflows/liquers-py-release.yml:193`
  - `scripts/test_check_wheel.py:35`
  - `tests/typing/stubtest-allowlist.txt` (the `_native` entries)
  - `tests/test_STUBS_declarations.py` (the stubtest arguments, `test_native_names_are_declared`,
    `test_declaration_files_exist`)
  - `tests/test_RECORDS_columns.py`
  - `tests/package/test_PACKAGE_wheel.py`
- Change:
  - `module-name = "_liquers"` and `python-packages = ["liquers"]`.
  - `#[pymodule(name = "_liquers", gil_used = false)]`; the Rust function keeps its name.
  - `_liquers/__init__.py` is a module doc plus `from ._liquers import *` and
    `__all__ = _liquers.__all__`.
  - Class modules stay `"liquers"` in this step.
- Depends on: none.
- Proof: `bash scripts/test-liquers-py.sh` under 3.14 (the whole suite, STUBS included), and
  `python3 -m pytest scripts/test_check_wheel.py`. `test_LAYOUT06_native_package_is_private` is
  added here and passes.
- Rollback: revert the commit.

### Step 2: public submodules, class module names, `liquers.error`
- Files / symbols:
  - new `liquers/{query, commands, metadata, dependencies, plan, recipes, store, environment, assets, error, data_nodes, pipeline, template, records, server, lui}.py`
  - `liquers/__init__.py` (rewritten: it imports `liquers.error` first, then the submodules, then
    their names; `__all__` keeps its content; `_ERROR_SUBCLASSES` becomes an alias)
  - `liquers/_errors.py` (the `__module__` values at lines 83, 111 and 126)
  - every `#[pyclass(…, module = "liquers")]` in `liquers-py/src/` (75), set from the Phase 1 map
  - new `tests/test_LAYOUT_submodules.py`
  - `tests/test_OBJECT_conformance.py` (OBJECT08)
- Change: as in Phase 2 *Integration Points* (Errors, Public modules). Each new file gets a
  first-version module doc; the finished doc comes in Step 4.
- Depends on: Step 1.
- Proof: `bash scripts/test-liquers-py.sh tests/test_LAYOUT_submodules.py
  tests/test_OBJECT_conformance.py tests/test_ERROR_errors.py`, then the whole suite except STUBS
  (`--deselect tests/test_STUBS_declarations.py`). Stubs still declare classes in
  `__init__.pyi`, so stubtest fails until Step 3; Steps 2 and 3 are therefore committed together,
  and Step 3's proof covers both.
- Rollback: revert Steps 2 and 3 together.

### Step 3: split the stubs per submodule
- Files / symbols:
  - `liquers/__init__.pyi`, split into `liquers/<sub>.pyi` with the stub-only helpers placed as in
    Phase 2
  - the error declarations (`__init__.pyi:1670` onwards) move to `error.pyi`
  - `tests/typing/stubtest-allowlist.txt` (`liquers.environment.Environment.live_handle_count`)
  - `tests/test_STUBS_declarations.py` (`remaining()` regex, `test_stubtest_reports_drift`,
    `stub_classes`, `test_error_classes_are_declared`)
  - new `tests/typing/good/submodules.py`
  - `tests/package/test_PACKAGE_wheel.py` (the submodule stubs)
- Change: declarations move with their names; imports are added between the stubs.
  `__init__.pyi` binds the submodules and re-exports every name.
- Depends on: Step 2.
- Proof: `bash scripts/test-liquers-py.sh` under 3.14 (`test_STUBS_declarations.py` runs only
  there), then the whole suite.
- Rollback: revert Steps 2 and 3.

### Step 4: API reference per submodule and module docs
- Files / symbols:
  - `tests/test_DOCS_api_reference.py`
    (`test_DOCS04_api_reference_has_a_page_per_submodule`)
  - `tests/test_LAYOUT_submodules.py` (`test_LAYOUT07_submodules_have_module_docs`,
    `test_LAYOUT08_private_helpers_import`)
  - module docs of the sixteen submodules, `_backend.py`, `_liquers/__init__.py`,
    `liquers/__init__.py` (module map; "never `_liquers`") and `liquers-py/src/lib.rs` `//!`
- Change: module docs follow `API_DOCUMENTATION_GUIDE.md` §3. Each names its Rust counterpart
  and has a doctest.
- Depends on: Step 3.
- Proof: `python3 scripts/check_api_docs.py` on each touched file reports nothing;
  `bash scripts/test-liquers-py.sh tests/test_DOCS_api_reference.py tests/test_DOCS_docstrings.py
  tests/test_LAYOUT_submodules.py`; `bash scripts/build-liquers-py-docs.sh`.
- Rollback: revert the commit.

### Step 5: documents
- Files / symbols: the Phase 2 *Documentation Architecture* table:
  - `specs/reference/PYTHON_INTEGRATION.md` §1, §6, §13, §14, §18
  - `specs/guides/PYTHON_GUIDE.md` §2
  - `docs/guides/python/01-introduction.md`
  - `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`
  - `specs/guides/API_DOCUMENTATION_GUIDE.md:139`
  - `liquers-py/README.md`
- Change: as in the table. Every `reference/` and `guides/` document gets a `## History` row and a
  `reviewed:` bump (§9.2), and the user-guide chapter gets a `HISTORY.md` row.
- Depends on: Step 4.
- Proof: `python3 scripts/docs_index.py --check`; `cargo run -p liquers-docs -- check`;
  `bash scripts/test-liquers-py.sh tests/test_GUIDE_examples.py`. There are no
  `_native` mentions outside `specs/archive/` and the design records:
  `grep -rn "_native" --exclude-dir=target --exclude-dir=archive --exclude-dir=design .` lists
  only lines that are not about the Python layout.
- Rollback: revert the commit.

## Testing Plan

During each step, run that step's proof and nothing more. After Step 5, run the whole suite once
under 3.14 (typing leg) and once under 3.13 with `bash scripts/test-liquers-py.sh`, plus
`cargo test -p liquers-py --no-default-features --features debug-handles --lib`, which the script
runs. A full wheel build (`maturin build`) checks the R1 layout and the wheel contents
(`tests/package/`). No `cfg(feature)` or optional dependency changes, so the build matrix is not
needed. CI on the PR runs `liquers-py-tests.yml` on 3.10, 3.14 and 3.14t.

## Rollback Plan

Revert the series. Steps 1 and 4–5 can each be reverted alone. Steps 2 and 3 go together: a class
named `liquers.query` needs its stub in `query.pyi`. If only Step 1 lands, the package works with
the flat API over `_liquers`.

## Documentation Updates

These are the documents of Step 5 and the module docs of Step 4, checked with
`scripts/check_api_docs.py` (`API_DOCUMENTATION_GUIDE.md` §10). Regenerated files:
`specs/index.csv`, `specs/index.md` and the `specs/README.md` blocks, all through
`scripts/docs_index.py`. `specs/command_registry.yaml` is untouched, because no command changes.
History rows and `reviewed:` bumps go on each changed `reference/` or `guides/` document.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
