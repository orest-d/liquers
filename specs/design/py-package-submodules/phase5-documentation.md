# Phase 5: Documentation - py-package-submodules

## Completion Preconditions

- [x] Implementation is finished and validated: the full suite on CPython 3.14 and 3.13, and the
  wheel check (see Validation)
- [x] All user comments are answered or incorporated: Orest's gate answers of 2026-11-14 are
  Decisions D1-D5 of Phase 1; nothing open since "proceed all" (2026-11-15)
- [x] All review comments are answered or incorporated: Phase 1 reviewed by the author, Phases 2-4
  and this phase reviewed twice each (conformity and records; accuracy against the code), findings
  applied
- [x] Documentation is consistent with the implemented and tested behavior: the documents below
  were reviewed against the code and tests, not the design
- [x] Documentation is included in the implementation PR (PR #272)

## Implementation Summary

Built as approved, in the four commits of Phase 4 *Progress*:

- The native extension is the private package `_liquers` (`python/_liquers/__init__.py`
  re-exports the compiled `_liquers._liquers`). `liquers/_backend.py` is the only module that
  imports it, the seam a pure-Python fallback would plug into (`PY-HAS-NO-PURE-PYTHON-FALLBACK`).
- Sixteen public submodules, `liquers.query` to `liquers.lui`, each import their names from
  `_backend` and own them in `__all__`; `liquers/__init__.py` re-exports all of them, so every
  top-level name of `liquers.__all__` is unchanged (`test_LAYOUT02`). `liquers.lui` and
  `liquers.data_nodes` keep `lq.ui` and `lq.data_node` functions in either import order
  (`test_LAYOUT04`).
- Every `#[pyclass]` reports its submodule, `QuerySegment` included, and the error classes report
  `liquers.error` and still pickle as classes (`test_LAYOUT05`, `test_OBJECT08`).
- The stubs are split, one `.pyi` per submodule; stubtest runs on `liquers _liquers`.
- Each submodule, `_backend.py` and `_liquers/__init__.py` has a full module doc with an executed
  example; the doctest run covers both packages.

Changed from Phase 2, with the reason:

- **Stub-only helpers are defined in every stub that uses them** (`_P`, `_R`, `_Decorator` in
  `commands.pyi` and `environment.pyi`; `_MetadataLike` in `store.pyi`), not imported from a home
  stub. pdoc executes the `.pyi` files, and a private name imported across stubs failed there.
  Aliases that name a class (`_Fields`, `_MetadataLike`) sit after the classes, for the same
  reason.
- **`scripts/build-liquers-py-docs.sh` passes the submodules to pdoc.** pdoc follows only the
  submodules that `__all__` names, and `liquers.__all__` must not name them (`test_LAYOUT03`), so
  the script lists the public submodules itself and checks every page;
  `test_DOCS04_api_reference_has_a_page_per_submodule` runs the script rather than pdoc.
- **`_liquers/__init__.pyi` gained `Server` and the five UI classes in `__all__`**, which
  `_native.pyi` had left out; `_backend.py` imports them.
- **The user-guide paragraph went to chapter 2 §2.4 "What is in the package"**, not chapter 1,
  because that section already lists the names and chapter 1 says nothing about the layout.

- **`specs/README.md` gains a capability-map entry**, where Phase 2 planned no link: Phase 5
  requires the map to point to the current reference once one exists.
- **`liquers-py/README.md`: the *Layout* section changed, not an "OBJECT08 row"**, which Phase 2
  named but which does not exist (the matrix has one `OBJECT` line, still right).

Nothing was omitted.

## Documentation Delivered

### New Reference Documents
None: `reference/PYTHON_INTEGRATION.md` §1 holds the layout.

### New Guide Documents
None: `guides/PYTHON_GUIDE.md` §2 holds the module table.

### Existing Documents Reviewed or Updated
`affects_docs`, each with a History row and `reviewed: 2026-11-15`:
- `specs/reference/PYTHON_INTEGRATION.md`: §1 layout and module table, §6, §13, §14, §18.
- `specs/guides/PYTHON_GUIDE.md`: §2 rewritten (submodules, Rust counterparts, the two names).
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §6 Python: the stub lesson and the maturin lesson.
- `specs/guides/API_DOCUMENTATION_GUIDE.md`: the Python signature line names the submodule stub.
- `liquers-py/README.md` *Layout*: how a class is added now (no History section).
- `docs/guides/python/02-installing.md` §2.4 (see below).

Discarded candidate: `specs/reference/COMMAND_DECLARATION.md` shares the `py` area but says
nothing about the package layout.

### Module Docs
The sixteen `liquers/<sub>.py`, `liquers/__init__.py`, `liquers/_backend.py`,
`_liquers/__init__.py` and `liquers-py/src/lib.rs`: `check_api_docs.py --strict` reports nothing
for any of them.

### User-Guide Chapters
`docs_index.py chapters-for` lists `01`-`15` and `c-glossary` of the Python guide, reviewed
after main's chapter rewrite (PR #270) was merged in, keeping its text. On Orest's ask
(2026-11-15), the examples use the public submodules where a chapter is about their classes, and
the text changes only around them:
- `02-installing.md` §2.4: the submodules, with an example.
- `03` (`liquers.query`, `liquers.pipeline`), `04` (`liquers.data_nodes`), `06` (`liquers.store`),
  `07` (`liquers.recipes`), `09` and `13` (`liquers.records`), `15` (`liquers.error`): the
  examples import from the submodule, and the first mention names it. API links point to the
  submodule pages.
- `c-glossary.md` C.2 gains a module column.
- The others use only the entry points (`lq.Environment`, `lq.evaluate`, the decorators, the
  error classes): reviewed, unchanged. No example imports `_liquers`. Every example runs, and each
  chapter has a `HISTORY.md` row.

### Links and Capability Map
`specs/README.md`: new entry "Python package layout", linking `PYTHON_INTEGRATION.md` §1 and §13
and `PYTHON_GUIDE.md` §2.

## Issues Filed

- `PY-HAS-NO-PURE-PYTHON-FALLBACK`: the fallback the seam leaves room for (Orest, Phase 1).
- `PY-CANNOT-INSPECT-THE-DEPENDENCY-MANAGER`: the wrapper `liquers.dependencies` will hold.
- `PY-ERROR-INSTANCES-CANNOT-BE-PICKLED`: found in Phase 3, pre-existing.

`PY-API-IS-ONE-FLAT-MODULE` is closed with a resolution note. `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED`
and `PY-ABI3T-WHEEL-NOT-USED` gain notes on the new paths.

## Important Learning

- stubtest and pdoc follow `__module__` and `__all__`, not where a class is defined; the module a
  class reports decides where its stub lives and which page documents it.
- maturin refuses a bare top-level extension in a mixed project; a private package that
  re-exports it works. Recorded in `LANGUAGE-INTEGRATION_GUIDE.md` §6 Python.
- pdoc executes stubs: a stub must not import a private name from another stub, and an alias must
  follow the classes it names.

## Conformance and Remaining Work

Requested (a structure like the Rust modules), approved (D1-D5: `_liquers` private, room for a
fallback, classes report their submodule, Rust names except `lui` and `data_nodes`, every
top-level name kept) and implemented agree, with the four changes above. Nothing remains in this
design; follow-up work is in the issues above.

## Validation

- `PYTHON=python3.14 bash scripts/test-liquers-py.sh`: 686 passed, 21 skipped, 1 xpassed; 19
  doctests; pdoc; 50 Rust unit tests. `PYTHON=python3.13`: 673 passed, 34 skipped, 1 xpassed; the
  same doctests and Rust tests.
- Wheel: `maturin build` (cp313), a fresh venv, `LIQUERS_PY_WHEEL_TEST=1 RELEASE_VERSION=0.1.0
  pytest liquers-py/tests/package`: 7 passed, 5 skipped. The third-party notices were a placeholder
  file, since `cargo about` runs only in CI.
- The final runs found three wrong doctests in the new module docs (`assets`, `environment`,
  `dependencies`), an allowlist entry still naming `liquers.Environment`, and pdoc writing no
  submodule pages; all three are fixed in the last commit.
- `check_api_docs.py --strict` on every module above; `python3 -m pytest scripts/`.
- `docs_index.py --check`: 0 errors. `cargo run -p liquers-docs -- check`: 0 errors, 0 warnings.
- `grep -rn "liquers\._native\|_native\.pyi"` outside `archive/` and `design/` finds only
  dated text in issues and LAYOUT06's assertion that `liquers._native` is gone.
