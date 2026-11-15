# Phase 3: Examples & Use-cases — py-package-submodules

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Finding the query API by module | `from liquers.query import …`, same objects as `lq.X` | AC-1, AC-2 |
| 2 | Example | UI and data nodes keep their top-level functions | `liquers.lui`, `liquers.data_nodes`, no shadowing | AC-3 |
| 3 | Test | `tests/test_LAYOUT_submodules.py::test_LAYOUT01_submodule_imports_in_a_fresh_interpreter` | import from a submodule first | AC-1 |
| 4 | Test | `…::test_LAYOUT02_top_level_names_are_unchanged` | today's names, frozen | AC-2 |
| 5 | Test | `…::test_LAYOUT03_each_name_has_one_home` | identity and partition | AC-2, AC-5 |
| 6 | Test | `…::test_LAYOUT04_functions_are_not_shadowed` | both import orders | AC-3 |
| 7 | Test | `…::test_LAYOUT05_classes_report_their_submodule` and `test_OBJECT08_…` | `__module__` | AC-4 |
| 8 | Test | `…::test_LAYOUT06_native_package_is_private` and `test_PACKAGE_wheel.py::test_wheel_contents` | `_liquers`, the seam | AC-6 |
| 9 | Test | `test_STUBS_declarations.py::test_stubtest_passes`, `test_declaration_files_exist`, `test_good_samples_pass` | stubs per submodule | AC-7 |
| 10 | Test | `test_DOCS_api_reference.py::test_DOCS04_api_reference_has_a_page_per_submodule`, `…_lists_every_public_name`, `test_LAYOUT07_submodules_have_module_docs` | reference | AC-8 |
| 11 | Test | `…::test_LAYOUT08_private_helpers_import` plus the existing suites | private modules | AC-9 |

## Example 1: Finding the query API by module

A reader who knows `liquers_core::query` looks for the Python counterpart. The query classes are
in one module, with a module doc and a reference page. They are the same objects as the top-level
names, so code written either way mixes freely.

```python
import liquers as lq
from liquers.query import Key, Query, parse

q = parse("-R/data/x.csv/-/ns-pl/from_csv")
assert isinstance(q, Query)
assert Key is lq.Key and lq.query.Key is lq.Key
assert type(q).__module__ == "liquers.query"
assert "Key" in lq.query.__all__
```

Expected: every assertion holds; `help(liquers.query)` starts with the module doc that names
`liquers_core::query`. The query `-R/data/x.csv/-/ns-pl/from_csv` is checked with `liquers-validate`
(a `GetAsset` of `data/x.csv`, then the action `from_csv` in namespace `pl`); it is only parsed here, never evaluated.

## Example 2: UI and data nodes keep their top-level functions

`ui` and `data_node` are both a top-level function and the name of a Rust module. The modules are
named `lui` and `data_nodes` instead (D2), so importing them changes nothing at the top level.

```python
import liquers.lui
import liquers.data_nodes
import liquers as lq

assert callable(lq.ui) and lq.lui.ui is lq.ui
node = lq.data_node({"a": 1})
assert node.kind == lq.data_nodes.NodeKind.Map
assert lq.data_nodes.data_node is lq.data_node
```

## Edge and Error Cases

- **A submodule imported before `liquers`.** `from liquers.error import KeyNotFoundError` in a
  fresh interpreter has to run `liquers/__init__.py` first, then `error.py`, without a cycle →
  LAYOUT01 runs each of the sixteen first imports in its own interpreter.
- **A Rust error raised before any Python import.** `error_to_pyerr` imports `liquers._errors`,
  which loads the package. That must still give the generated subclass → the existing
  `test_ERROR_errors.py` suite runs with `liquers.error` installing the classes.
- **Pickled error classes.** `pickle.loads(pickle.dumps(lq.KeyNotFoundError)) is
  lq.KeyNotFoundError` resolves through `liquers.error` → LAYOUT05. Instances do not pickle, today
  or after (`PY-ERROR-INSTANCES-CANNOT-BE-PICKLED`).
- **Debug-only native names.** `_liquers.live_record_handle_count` exists only in a debug-handles
  build → `test_RECORDS_columns.py` keeps its `hasattr` skip, with `import _liquers`; the stubtest
  allowlist keeps only used entries (`test_stubtest_passes`).
- **Native module missing.** There is no fallback yet, so `import liquers` raises the original
  `ImportError` from `liquers._backend`. Not tested: it would need a broken installation.

## Test Plan

New file `liquers-py/tests/test_LAYOUT_submodules.py`. The names `LAYOUT01` to `LAYOUT08` are
local to this design and are not conformance IDs of `LANGUAGE-INTEGRATION_GUIDE.md`. Its
`SUBMODULES` constant lists the sixteen modules once.

- `test_LAYOUT01_submodule_imports_in_a_fresh_interpreter` (AC-1): for each submodule SUB and the first name NAME of its `__all__`, runs a
  subprocess `python -c "from liquers.SUB import NAME; import liquers; assert
  liquers.SUB.NAME is liquers.NAME"`, as `[sys.executable, "-c", …]` with `capture_output` and a
  timeout (as `tests/test_DATANODE_views.py` does); for `liquers.error` it imports
  `KeyNotFoundError`, a generated class.
- `test_LAYOUT02_top_level_names_are_unchanged` (AC-2): `set(lq.__all__)` equals a frozen literal
  of today's 89 names plus `lq.error._ERROR_SUBCLASSES`. `lq.__version__` still exists. Whichever
  of this design, PR #273 (`configure`) and PR #265 (`import_module`) lands later updates the
  literal.
- `test_LAYOUT03_each_name_has_one_home` (AC-2, AC-5): for each submodule and each name in its
  `__all__`, `getattr(lq, name) is getattr(sub, name)`. The `__all__` lists are pairwise disjoint,
  their union is `set(lq.__all__)`, and no submodule name is in `lq.__all__`.
- `test_LAYOUT04_functions_are_not_shadowed` (AC-3): in subprocesses, `import liquers.lui,
  liquers.data_nodes` before and after `import liquers`. In both orders `lq.ui` and `lq.data_node`
  are functions, and `liquers.lui.ui is lq.ui`.
- `test_LAYOUT05_classes_report_their_submodule` (AC-4): every class in a submodule's `__all__`
  reports `__module__ == sub.__name__` (errors: `"liquers.error"`), and so does `QuerySegment`
  (`liquers.query`). A generated error class survives a pickle round trip.
- `test_OBJECT08_wrappers_follow_naming_and_ownership_conventions` (AC-4, changed): `__module__`
  is one of the sixteen submodule names.
- `test_LAYOUT06_native_package_is_private` (AC-6): `importlib.util.find_spec("liquers._native")`
  is `None`, and `_liquers.Key is lq.Key`. Reading every `liquers/*.py` with `ast`, only
  `_backend.py` imports `_liquers`, and no public module imports a name from it directly.
- `test_PACKAGE_wheel.py::test_wheel_contents` (AC-6, changed): the wheel lists
  `_liquers/__init__.py`, `_liquers/__init__.pyi`, `_liquers/py.typed`, `liquers/py.typed` and one
  `liquers/<sub>.pyi` per submodule. No file starts with `liquers/_native`.
- `test_STUBS_declarations.py` (AC-7): `test_declaration_files_exist` expects the new stub files;
  `test_stubtest_passes` runs stubtest on `liquers _liquers` with the updated allowlist;
  `test_good_samples_pass` gets a new sample `tests/typing/good/submodules.py`
  (`from liquers.records import RecordView`, `from liquers.lui import UI`, `liquers.query.Key`
  through an `import liquers`); `test_stubtest_reports_drift` and `test_error_classes_are_declared`
  are retargeted as in Phase 2.
- `test_DOCS_api_reference.py::test_DOCS04_api_reference_has_a_page_per_submodule` (AC-8): pdoc
  writes `liquers/<sub>.html` for each submodule, with an `id` for each name of its `__all__`.
  `test_DOCS04_api_reference_lists_every_public_name` stays as it is.
- `test_LAYOUT07_submodules_have_module_docs` (AC-8): `scripts/check_api_docs.py --strict` exits 0
  with no output for each submodule file and `_liquers/__init__.py` (collected after Phase 2's
  change); `_backend.py` is checked under the private-module rule, and its five parts are a review
  item of Step 4. The doctests of all of them run in `scripts/test-liquers-py.sh`.
- `test_LAYOUT08_private_helpers_import` (AC-9): `liquers._ui._run_as_command`,
  `liquers._env._awaiting`, `liquers._commands`, `liquers._errors`, `liquers._records` and
  `liquers._factories` import, which are the names the Rust code imports. The behaviour behind
  them stays covered by the existing suites (`test_COMMAND_registration.py`,
  `test_STORE_python_protocol.py`, `test_RECORDS_arrow.py`, `test_UIUSE_model.py`,
  `test_ERROR_errors.py`). The whole suite runs unchanged apart from the files named above.

Command to run: `PYTHON=python3.14 bash scripts/test-liquers-py.sh` (CPython 3.14 with the GIL, the
typing leg, which runs the STUBS suite; the script always runs the whole suite, doctests, pdoc and
the crate's Rust tests). Then `PYTHON=python3.13 bash scripts/test-liquers-py.sh` once. A single
file between runs: `target/liquers-py-venv-cpython3.14/bin/python -m pytest
liquers-py/tests/test_LAYOUT_submodules.py`.

## Learning Log

- maturin (1.16) will not build a top-level extension module beside a `python-source` package.
  The working form is a private package holding the extension (Phase 2 R1).
- pdoc documents the submodules of a package without being told. It gives every `__all__` name an
  `id`, re-exports included, but gives a submodule none.
- stubtest names a class by the module it reports, not by the module that re-exports it, and it
  does not check star-copied names in a module that has no `__all__`.
