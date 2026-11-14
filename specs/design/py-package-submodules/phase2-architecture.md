# Phase 2: Solution & Architecture — py-package-submodules

## Overview

Sixteen thin Python modules under `liquers/` re-export native names. One private module,
`liquers/_backend.py`, is the only place that imports the native package `_liquers`. Every class
names its public home in `#[pyclass(module = "liquers.<sub>")]`. `liquers-py/src/lib.rs` keeps one
flat native module.

These points were settled by building a toy crate with PyO3 0.30.1 (the repository's version) and
maturin 1.16.0 on 2026-11-14:

- **R1 (Q7), maturin layout.** When `python-source` is set, maturin refuses a bare top-level
  extension. It does accept `module-name = "_liquers"` with `python-packages = ["liquers"]` when
  `python/_liquers/` is a package. The wheel then holds `_liquers/__init__.py`,
  `_liquers/_liquers.<abi>.so` and `liquers/`, and `maturin develop` builds into `python/_liquers/`.
  So `_liquers` (D4) is a private *package* that re-exports the compiled `_liquers._liquers`. As a
  package it can carry its own `py.typed` and `__init__.pyi` (PEP 561).
- **R2 (Q5), pdoc.** `pdoc liquers` writes `liquers.html` with an `id` for every `__all__` name,
  re-exports included, plus one page per submodule. DOCS04 therefore stands. Submodule names stay
  out of `__all__`, because pdoc gives them no `id`.
- **R3, stubtest.** stubtest looks a class up in the stub of the module it reports (`toyq.query.Key`
  in `toyq/query.pyi`), and it checks both packages in one run. A star-copied name with no `__all__`
  is checked nowhere, and that makes allowlist entries unused (see *Integration Points*, Stubs).

Rejected: PyO3 `add_submodule`, which needs hand registration in `sys.modules` and which stubtest
and pdoc see poorly; a bare top-level `.so`, which R1 rules out; keeping `liquers._native`, which
D4 rules out; a callable module, which D2 rules out.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `PY-API-IS-ONE-FLAT-MODULE` | draft | P3 | the source | — | no | this design |
| `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` | draft | P3 | adds a query class | no | no | whichever lands second exports `QuerySegment` from `liquers.query`, its stub helper `_QuerySegment` included |
| `PY-REPRS-SHOW-RUST-DEBUG-FORMAT` | draft | P3 | default reprs show `liquers.<sub>.X` | no | no | hand-written reprs (`<liquers.Server …>`) keep the valid top-level form |
| `PY-NO-PUBLISHED-WHEELS` | in_progress | P2 | the first release carries the layout | no | no | land before the first release |
| `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` | draft | P3 | 18 new module docs, 2 changed | no | no | the new docs follow the rules |

## Interfaces

- Rust: no new type or function. `liquers-py/src/lib.rs` keeps `fn _native(…)` and adds
  `#[pymodule(name = "_liquers", gil_used = false)]`, with the same body. Each of the 75
  `#[pyclass(…, module = "liquers")]` attributes in `liquers-py/src/` gets its submodule from the
  Phase 1 map. No `ExtValue` change, no command change, and no sync/async change.
- Python: the public modules `liquers.{query, commands, metadata, dependencies, plan, recipes,
  store, environment, assets, error, data_nodes, pipeline, template, records, server, lui}`, each
  with an explicit `__all__` taken from the map; the private `liquers._backend`; and the package
  `_liquers`. No public name is added, removed or renamed. Names that other designs add later go
  into `liquers.environment` (Phase 1 *Design Dependencies*): `configure` from PR #273 and
  `import_module` from PR #265.

## Integration Points

- **Build.**
  - `liquers-py/pyproject.toml` `[tool.maturin]`: `module-name = "_liquers"` and
    `python-packages = ["liquers"]`, with the comment above them updated.
  - `.github/workflows/liquers-py-release.yml:193`: the Windows dependency check's glob changes
    from `unpacked/liquers/*.pyd` to `unpacked/_liquers/*.pyd`. Today's glob would match nothing,
    and the check would test nothing.
  - `.gitignore:27-28` ignores `liquers-py/python/_liquers/_liquers*.{so,pyd}`.
  - `scripts/check-wheel.py` and `scripts/build-liquers-py-docs.sh` stay unchanged: neither depends
    on paths, and pdoc finds the submodules itself.
- **`python/_liquers/__init__.py`** (new) holds a module doc, `from ._liquers import *` and
  `__all__ = _liquers.__all__`. PyO3 adds every `m.add` name to that `__all__`, so `__version__`
  and the debug-only counts come along.
- **Seam.** `liquers/_backend.py` (new) imports each native name explicitly from `_liquers` and
  declares `__all__`. Its module doc states the fallback contract: the same names, and the same
  `__module__` values. `_env.py:7` and `_commands.py:40` import from `liquers._backend` instead of
  `liquers._native`. No other helper imports a native name.
- **Errors.**
  - `_errors.py` sets `__module__ = "liquers.error"` on `LiquersError` and on every class that
    `_install` generates.
  - The `_install(ErrorType)` call moves from `liquers/__init__.py` to `liquers/error.py`. That file
    binds the classes as module attributes (`globals().update`), keeps `_ERROR_SUBCLASSES`, and
    builds `__all__` from it.
  - `liquers/__init__.py` imports `liquers.error` first, then the other submodules, then their
    names. It keeps `_ERROR_SUBCLASSES` as an alias and `__all__` with today's content; `__version__`
    comes from `_backend`.
  - The Rust `error_to_pyerr` imports `liquers._errors` (`src/error.rs:230,364`), which now loads
    the whole `liquers` package even when only `_liquers` was imported. That is harmless, because
    the package has no import-time native call besides `_install`.
- **Public modules.** There is one file per row of the map, each with a module doc. Most import
  from `_backend`. `environment.py` also takes `default_environment`, `evaluate`, `evaluate_async`
  and `require` from `_env` and `_extras`; `server.py` takes `serve` and `serve_async`; `lui.py`
  takes `ui`; `commands.py` takes the decorators.
- **Stubs.**
  - Declarations move from `liquers/__init__.pyi` into one `.pyi` per public module, unchanged
    except for imports. The stubs import from each other (`from liquers.query import Key`).
  - The stub-only helpers go where they are used: `_P`, `_R` and `_Decorator` to `commands.pyi`;
    `_Fields` to `records.pyi`; `_MetadataLike` to `metadata.pyi`; `_QuerySegment` to `query.pyi`. A
    helper used in several stubs is imported from its home stub. No stub-only module is added,
    because stubtest would report one that has no runtime module.
  - `__init__.pyi` binds the submodules (`from liquers import query as query, …`) and re-exports
    every name (`from liquers.query import Key as Key`, …).
  - `liquers/_native.pyi` becomes `_liquers/__init__.pyi`, and `_backend.pyi` re-exports. Both
    packages carry `py.typed`.
  - `tests/typing/stubtest-allowlist.txt`: `liquers.Environment.live_handle_count` becomes
    `liquers.environment.Environment.live_handle_count`. `liquers._native.live_record_handle_count`
    and `liquers._native.__all__` become `_liquers.…`. Phase 4 then keeps only the entries a
    debug-handles stubtest run reports as used.
- **Tests.**
  - `tests/test_STUBS_declarations.py`:
    - stubtest runs on `liquers _liquers`.
    - `test_native_names_are_declared` reads `_liquers` against `_liquers/__init__.pyi`.
    - `test_declaration_files_exist` expects the new stub files.
    - `test_stubtest_reports_drift` edits `class Environment:` in `environment.pyi` and expects
      `liquers.environment.Environment.evaluate_state`.
    - `stub_classes` and `test_error_classes_are_declared` read `error.pyi` and
      `liquers.error._ERROR_SUBCLASSES`.
    - `remaining()` matches `^error: _?liquers\.(?:(?:query|commands|…|lui)\.)?(\w+)`, with the
      sixteen names spelled out.
  - `test_OBJECT08` asserts that `__module__` is a public submodule of `liquers`.
  - `tests/package/test_PACKAGE_wheel.py` expects `_liquers/__init__.pyi`, `_liquers/py.typed` and
    the submodule stubs.
  - `test_RECORDS_columns.py` uses `import _liquers`.
  - `scripts/test_check_wheel.py:35` renames its fixture path.
  - AC-8's per-submodule pages get their own test in Phase 3, because the docs script checks only
    `liquers.html`.

## Error Handling

There is no new failure case and no new `Error` constructor. An `ImportError` from `_liquers`
(native library missing or broken) propagates unchanged from `liquers._backend`. A future fallback
would catch it there (`PY-HAS-NO-PURE-PYTHON-FALLBACK`). Pickling keeps working for exceptions:
`LiquersError` and its subclasses pickle by `__module__` and name, so `liquers.error` binds them.
The other classes cannot be pickled, as today.

## Relevant Commands

None. No command or namespace is involved.

## Documentation Architecture

| Document | Kind | Change |
|---|---|---|
| `specs/reference/PYTHON_INTEGRATION.md` | reference | §1 Layout (`_liquers`, `_backend`, the submodules, the `__module__` rule), §6 errors' module, §13 stub layout, §14 `liquers.data_nodes`, §18 `liquers.lui` |
| `specs/guides/PYTHON_GUIDE.md` | guide | §2 "What is in the package" rewritten as the package layout: the module map with its Rust counterparts, the `_liquers` row, `lq.X` still the style |
| `docs/guides/python/01-introduction.md` | user guide | a paragraph and a table on the package layout |
| `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | the lesson on `module = "liquers"` and stubs, restated for submodules |
| `specs/guides/API_DOCUMENTATION_GUIDE.md` | guide | line 139 names `_native.pyi`; change it to `_liquers/__init__.pyi` |
| `liquers-py/README.md` | readme | the OBJECT08 row, and each mention of `_native` |

Module docs, each with all five parts of `API_DOCUMENTATION_GUIDE.md` §3:
- New: the sixteen public modules (contents, Rust counterpart, a doctest), `_backend.py` (with the
  fallback contract as a related concept), and `_liquers/__init__.py`.
- Changed: `liquers/__init__.py`, which gains the module map and whose text "never `_native`"
  (lines 3, 31) becomes "never `_liquers`"; and `liquers-py/src/lib.rs` `//!` and the doc comment
  at line 76, which now name `_liquers`.

`specs/README.md` gains no link.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-py/src/` (75 attributes, `lib.rs`); `pyproject.toml`; `python/liquers/` (17 new, 4 changed: `__init__.py`, `_env.py`, `_commands.py`, `_errors.py`); `python/_liquers/` (new); stubs (~1,700 lines moved); the release workflow; `.gitignore`; the documents above |
| Crates and workflows affected | `liquers-py` only; `liquers-py-tests.yml` unchanged; `liquers-py-release.yml` one glob |
| Existing tests likely to change | `test_STUBS_declarations.py`, `test_OBJECT_conformance.py` (OBJECT08), `test_PACKAGE_wheel.py`, `test_RECORDS_columns.py`, `scripts/test_check_wheel.py`. Every other test keeps `lq.X` (D5) |
| New validation | tests of AC-1 to AC-9 (Phase 3); stubtest over both packages; pdoc pages per submodule |
| Compatibility, data, concurrency, performance, security | `__module__` values change, before any release. Errors pickled before the change name `liquers.X`, which still resolves. Import adds about seventeen small module loads |
| Recovery | each Phase 4 step is one commit; reverting the series restores `liquers._native` |
| Certainty and open questions | High. R1–R3 hold on the toy crate, and the full wheel build is Phase 4's first proof. There are no import cycles: no helper imports `liquers` at module load (`_records.py:33` imports it inside a function), and AC-3's test imports each submodule first in a fresh interpreter. No open question |
