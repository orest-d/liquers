# Phase 2: Solution & Architecture — py-package-submodules

## Overview

Sixteen thin Python modules under `liquers/` re-export native names. One private module,
`liquers/_backend.py`, is the only place that imports the native package `_liquers`. Every class
names its public home in `#[pyclass(module = "liquers.<sub>")]`. `liquers-py/src/lib.rs` keeps one
flat native module, renamed `_liquers`.

Settled by building a toy PyO3 0.30.1 crate with maturin 1.16.0 (the repository's PyO3; the
current maturin) on 2026-11-14:

- **R1 (Phase 1 Q7) — maturin layout.** With `python-source = "python"`, maturin refuses a bare
  top-level extension ("the python module at `python/_toyq` does not exist"). It accepts
  `module-name = "_liquers"` with `python-packages = ["liquers"]` when `python/_liquers/` is a
  package. The wheel then holds `_liquers/__init__.py`, `_liquers/_liquers.<abi>.so` and
  `liquers/`, and `maturin develop` puts the library in `python/_liquers/`. So `_liquers` is a
  private *package* whose `__init__.py` re-exports the compiled `_liquers._liquers`. Being a
  package, it carries its own `py.typed` and `__init__.pyi` (PEP 561).
- **R2 (Phase 1 Q5) — pdoc.** `pdoc liquers` writes `liquers.html` with an entry (`id`) for each
  `__all__` name, re-exports included, and one page per submodule (`liquers/query.html`, …). So
  `test_DOCS04_api_reference_lists_every_public_name` stands as it is. The submodule names stay out
  of `__all__`, because pdoc gives them no `id`.
- **R3 — stubtest.** stubtest looks up a class that reports `liquers.query` in
  `liquers/query.pyi`, and checks `liquers` and `_liquers` in one run.

Rejected alternatives:
- PyO3 `add_submodule`: it needs hand registration in `sys.modules`, and stubtest and pdoc handle
  builtin modules poorly.
- A bare `_liquers.so` at the top level: maturin refuses it alongside a Python source tree (R1).
- Keeping `liquers._native`: Orest decided against it (D4).
- Callable modules: rejected in D2.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-API-IS-ONE-FLAT-MODULE` | draft | P3 | the source | — | no | this design |
| `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` | draft | P3 | adds a query class | no | no | whichever lands second exports `QuerySegment` from `liquers.query` |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | default reprs show `liquers.<sub>.X` | no | no | hand-written reprs (`<liquers.Server …>`) keep the top-level form, which is a valid path |
| `PY-NO-PUBLISHED-WHEELS` | in_progress | P2 | the release carries the layout | no | no | land before the first release |
| `PY-LIVE-HANDLE-COUNT-FALLS-DURING-A-TEST` | draft | P3 | its test reads `_native` | no | no | only the import changes |
| `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` | draft | P3 | 17 new module docs | no | no | the new docs follow the rules |

## Interfaces

- Rust: no new type or function. `liquers-py/src/lib.rs` changes from `#[pymodule(gil_used =
  false)] fn _native(m: &Bound<'_, PyModule>) -> PyResult<()>` to `#[pymodule(name = "_liquers",
  gil_used = false)] fn _liquers(m: &Bound<'_, PyModule>) -> PyResult<()>`, with the same body. The
  75 `#[pyclass(…, module = "liquers")]` attributes in `liquers-py/src/` take the submodule from
  the Phase 1 map. No `ExtValue` variant changes, and no command is added or changed. There are no
  sync/async changes.
- Python: new modules `liquers.{query, commands, metadata, dependencies, plan, recipes, store,
  environment, assets, error, data_nodes, pipeline, template, records, server, lui}`, each with an
  explicit `__all__` drawn from the map; the private `liquers._backend`; and the package
  `_liquers`. No public name is added, removed or renamed.

## Integration Points

- `liquers-py/pyproject.toml` `[tool.maturin]`: `module-name = "_liquers"`, `python-packages =
  ["liquers"]`.
- `liquers-py/python/_liquers/__init__.py` (new): contains only `from ._liquers import *` and the
  module doc.
- `liquers/_backend.py` (new): imports each native name explicitly from `_liquers`, with
  `__all__`. Its module doc states the contract a fallback must meet: the same names and the same
  `__module__` values.
- Private helpers: `_env.py` (`from liquers._native import UI, Environment, Server`) and
  `_commands.py` (`… import Environment, Link`) switch to `liquers._backend`. No other helper
  imports a native name. Rust code imports the helpers by name, as in Phase 1 AC-9, and those
  imports are unchanged.
- `_errors.py`: `LiquersError.__module__` and the `__module__` of the classes `_install` generates
  change from `"liquers"` to `"liquers.error"`. The `_install(ErrorType)` call moves from
  `__init__.py` to `liquers/error.py`, which builds its `__all__` from the returned dict.
- `liquers/__init__.py`: imports the sixteen submodules, then their names. `__all__` keeps today's
  content, and `__version__` comes from `_backend`.
- Stubs: `liquers/__init__.pyi` is split into one `.pyi` per public module. Declarations are moved,
  not rewritten, and `__init__.pyi` re-exports them (`from liquers.query import Key as Key`, …).
  `liquers/_native.pyi` becomes `_liquers/__init__.pyi`, `_backend.pyi` re-exports, and both
  packages carry `py.typed`. In `tests/typing/stubtest-allowlist.txt`, the `liquers._native`
  prefixes become `_liquers`.
- Tests and tooling:
  - `tests/test_STUBS_declarations.py` runs stubtest on `liquers _liquers`, reads runtime names
    from `_liquers`, and matches `^error: (?:_?liquers)\.(?:\w+\.)?(\w+)`.
  - `test_OBJECT08` asserts "a public submodule of `liquers`" instead of `== "liquers"`.
  - `tests/package/test_PACKAGE_wheel.py` expects `_liquers/__init__.pyi`, `_liquers/py.typed` and
    the submodule stubs.
  - `test_RECORDS_columns.py` imports `_liquers`.
  - `.gitignore` ignores `liquers-py/python/_liquers/_liquers*.so` and `*.pyd`.
  - `scripts/test_check_wheel.py` renames its fixture path.
  - `scripts/build-liquers-py-docs.sh` and the release workflow need no change: the workflow builds
    with maturin from `pyproject.toml`, and `scripts/check-wheel.py` checks linked libraries, not
    paths.

## Error Handling

There is no new failure case and no new `Error` constructor. One case is worth stating: an
`ImportError` from `_liquers` (a missing or broken native library) propagates unchanged from
`liquers._backend`. That is the point where a future fallback (`PY-HAS-NO-PURE-PYTHON-FALLBACK`)
would catch it.

## Relevant Commands

None. No command or namespace is involved.

## Documentation Architecture

| Document | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | §1 Layout: `_liquers`, `_backend`, the public submodules and their contents, the `__module__` rule; §13 stub layout; §14 `liquers.data_nodes`; §18 `liquers.lui` |
| `specs/guides/PYTHON_GUIDE.md` | guide | new "Package layout" section: the module map with Rust counterparts |
| `docs/guides/python/01-introduction.md` | user guide | a paragraph and table on the package layout |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | the lesson on `module = "liquers"` and stubs, restated for submodules |
| `liquers-py/README.md` | readme | OBJECT08 row; the `_native` mentions |
| `specs/guides/API_DOCUMENTATION_GUIDE.md` | guide | its example names `liquers._native`; change it to `_liquers` |

Module docs, with all five parts (`API_DOCUMENTATION_GUIDE.md` §3: purpose, main components,
where used, related concepts, an executed example):
- each of the sixteen public modules: purpose, contents and Rust counterpart; the example is a
  doctest;
- `_backend.py`: purpose, and the fallback contract as a related concept;
- `_liquers/__init__.py`;
- `liquers/__init__.py`: the module map is added to its main components;
- `liquers-py/src/lib.rs` `//!`: the module name `_liquers`, and that classes report their Python
  submodule.

`affects_docs` grows by `specs/guides/API_DOCUMENTATION_GUIDE.md`. `specs/README.md` gains no new
link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/src/` (75 attributes, `lib.rs`), `pyproject.toml`, `python/liquers/*` (16 new, 3 changed), `python/_liquers/` (new), stubs (~1,700 moved lines), 5 test files, `.gitignore`, `scripts/test_check_wheel.py`, the documents above |
| Crates and workflows affected | `liquers-py` only; `liquers-py-tests.yml` and the release workflow run unchanged |
| Existing tests likely to change | `test_OBJECT08`, `test_STUBS_declarations.py`, `test_PACKAGE_wheel.py`, `test_RECORDS_columns.py`; every other test keeps `lq.X` (D5) |
| New validation | per-submodule tests for AC-1 to AC-9 (Phase 3), stubtest over both packages, pdoc pages per submodule |
| Compatibility / data / concurrency / performance / security | `__module__` values change, but there is no release yet. Pickling is unaffected because the classes cannot be pickled. Import time rises by about sixteen small module loads. No data, concurrency or security change |
| Recovery | each Phase 4 step is one commit; reverting the series restores `liquers._native` |
| Certainty and open questions | high: R1–R3 were checked on a toy crate; the full wheel build is Phase 4's first proof. Import cycles are avoided because helpers import `_backend`, not `liquers`; AC-3's test imports each submodule first in a fresh interpreter. No open question |
