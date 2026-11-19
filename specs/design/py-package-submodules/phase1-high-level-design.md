# Phase 1: High-Level Design — py-package-submodules


## Purpose

The `liquers` package puts about 120 public names in one namespace (89 listed in `__init__.py`
plus the generated error classes), so a reader who knows the Rust crates' modules cannot use that
structure in Python. This design moves the native extension to a private top-level module
`_liquers` and gives `liquers` public submodules named after the Rust modules, each a thin Python
module over `_liquers`, so a pure-Python fallback can later replace `_liquers` behind them.

Requested by Orest on 2026-11-14; Phase 1 gate answers of the same day are recorded under
*Decisions*.

## Problem Example

```python
import liquers as lq
len(lq.__all__)                  # today: ~120 names: query, record, UI and error classes mixed
from liquers.query import Key    # today: ModuleNotFoundError: No module named 'liquers.query'
help(lq)                         # today: one alphabetical list of everything
```

What should happen: `from liquers.query import Key` works, `liquers.query.Key is lq.Key`,
`help(liquers.query)` shows the query classes with a module doc, and the reference has one page per
submodule. (`lq` is only the local alias of `import liquers as lq`, used by the guides; it is not
an installed name and cannot clash with another package.)

Module map. Each module takes the Rust module's name, except two whose Rust name is already a
top-level function (`lq.ui()`, `lq.data_node()`): those are `liquers.lui` (Liquers UI) and
`liquers.data_nodes`, and the functions keep working (*Decisions* D2).

| Python module | Contents | Rust counterpart |
|---|---|---|
| `liquers.query` | `Query`, `Key`, `ActionRequest`, `ActionParameter`, `ResourceName`, `Position`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySource`, `parse`, `parse_key`, `Link`, `link` | `liquers_core::query`, `parse` |
| `liquers.commands` | `command`, `first_command`, `CommandMetadata`, `CommandDefinition`, `CommandKey`, `CommandPreset`, `CommandMetadataRegistry`, `ArgumentInfo`, `ArgumentType`, `EnumArgument`, `EnumArgumentType` | `liquers_core::command_metadata`, `commands` |
| `liquers.metadata` | `Metadata`, `MetadataRecord`, `AssetInfo`, `Status`, `LogEntry`, `LogEntryKind`, `Version`, `Expires`, `ExpirationTime` | `liquers_core::metadata`, `expiration` |
| `liquers.dependencies` | `DependencyKey`, `DependencyRecord`, `DependencyRelation`, `PlanDependency`; later the dependency-manager wrapper (`PY-CANNOT-INSPECT-THE-DEPENDENCY-MANAGER`) | `liquers_core::dependencies` (the first two from `metadata`) |
| `liquers.plan` | `Plan`, `Step`, `ParameterValue`, `ResolvedParameterValues`, `build_plan` | `liquers_core::plan` |
| `liquers.recipes` | `Recipe`, `RecipeList`, `RecipeProviders` | `liquers_core::recipes` |
| `liquers.store` | `Store`, `StoreArgumentInfo` | `liquers_core::store` |
| `liquers.environment` | `Environment`, `Context`, `Issue`, `default_environment`, `evaluate`, `evaluate_async`, `require` | `liquers_core::context`, `environment_builder` |
| `liquers.assets` | `State`, `AssetRef` | `liquers_core::state`, `assets` |
| `liquers.error` | `Error`, `ErrorType`, `LiquersError` and its generated subclasses | `liquers_core::error` |
| `liquers.data_nodes` | `data_node`, `DataNode`, `NodeKind` | `liquers_core::data_node` |
| `liquers.pipeline` | `Pipeline`, `PipelineEditor`, `QueryEditor`, `ActionEditor` | `liquers_core::pipeline` |
| `liquers.template` | `RenderResult`, `Diagnostic` | `liquers-template`, `liquers_lib::template` |
| `liquers.records` | `RecordView`, `RecordSource`, `RecordStream`, `RecordChunk`, `RecordColumn`, `RecordBuffer`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin` | `liquers-records` |
| `liquers.server` | `serve`, `serve_async`, `Server` | `liquers-axum` |
| `liquers.lui` | `ui`, `UI`, `UIElement`, `UIElementInfo`, `UIStatusEvent`, `UISubscription` | `liquers_lib::ui` |

## Scope and Acceptance Criteria

In scope: renaming the native module `liquers._native` to the top-level `_liquers`; the public
submodules; each class's reported module; the type declarations; the API reference; the
documentation. Out of scope: the pure-Python fallback itself (`PY-HAS-NO-PURE-PYTHON-FALLBACK`),
the dependency-manager wrapper (`PY-CANNOT-INSPECT-THE-DEPENDENCY-MANAGER`), and store-backed
module import (`liquers-py-modules`, PR #265). The private Python modules (`_env`, `_commands`,
`_errors`, `_ui`, `_records`, `_factories`, `_extras`) stay inside `liquers`, because Rust code
imports them by name.

- **AC-1** Import from a submodule
  WHEN `from liquers.query import Key, parse` runs in a fresh interpreter
  THEN both names import, and `liquers.query.Key is liquers.Key`
- **AC-2** Top-level names kept
  WHEN a program uses any name of today's `liquers.__all__` as `liquers.X`
  THEN it gets the same object as `liquers.<submodule>.X`
- **AC-3** No collision with top-level functions
  WHEN `import liquers.lui` and `import liquers.data_nodes` run, in either order, before or after
  `import liquers`
  THEN `lq.ui` and `lq.data_node` are still the functions, and `liquers.lui.ui is lq.ui` and
  `liquers.data_nodes.data_node is lq.data_node`
- **AC-4** Classes report their submodule
  WHEN `type(lq.parse("a/b")).__module__` or `lq.KeyNotFoundError.__module__` is read
  THEN it is `"liquers.query"` or `"liquers.error"`: every public class reports the submodule that
  exports it
- **AC-5** One home per name
  WHEN the `__all__` lists of all public submodules are collected
  THEN each public name appears in exactly one of them, and their union equals `liquers.__all__`
  (the submodules themselves are attributes of `liquers`, not `__all__` entries)
- **AC-6** Native module is private and top-level
  WHEN the wheel is installed
  THEN it contains `_liquers` beside `liquers/` and no `liquers/_native`, and no public submodule
  imports `_liquers` except through the one import seam (`liquers/_backend.py`), which a fallback
  would replace
- **AC-7** Type declarations follow the submodules
  WHEN stubtest and pyright run over the installed package (`tests/test_STUBS_declarations.py`)
  THEN they pass with one `.pyi` per public submodule, and `from liquers.records import RecordView`
  is typed
- **AC-8** Reference has a page per submodule
  WHEN `scripts/build-liquers-py-docs.sh` runs
  THEN pdoc writes a page per public submodule, each with a module doc meeting
  `specs/guides/API_DOCUMENTATION_GUIDE.md` §3, and the `liquers` page lists every `__all__` name
- **AC-9** Private modules undisturbed
  WHEN a Python command, a Python store, a records conversion or an async UI wait runs
  THEN the Rust imports of `liquers._ui`, `liquers._env`, `liquers._commands`, `liquers._errors`,
  `liquers._records` and `liquers._factories` still resolve

## Core Interactions

Bindings only: the Python package `liquers` and the native module of `liquers-py`. No query,
store, command, asset or value-type behaviour changes.

## Crate Placement

`liquers-py` only: `liquers-py/src/` (class module names, the module name) and
`liquers-py/python/` (the packages `liquers` and `_liquers`, stubs). No other crate changes.

## Documentation Intent

- `specs/reference/PYTHON_INTEGRATION.md` §1 Layout (`_liquers`, the seam, the public submodules),
  the `__module__` rule, §13 the stub layout, §14 and §18 for `liquers.data_nodes` and `liquers.lui`.
- `specs/guides/PYTHON_GUIDE.md`: §2 "What is in the package" becomes the package layout, mapping
  submodules to Rust modules.
- `docs/guides/python/01-introduction.md`: the package layout. Examples keep
  `import liquers as lq`.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: the lesson "PyO3's `module = "liquers"` decides
  where the stubs go", updated to submodules.
- `liquers-py/README.md`: the OBJECT08 row.
- API docs: `liquers-py/python/liquers/__init__.py` (the package doc gains the submodule map), a
  module doc for each of the 16 public submodules and for `_backend.py`; `liquers-py/src/lib.rs`
  `//!` (module `_liquers`, classes report their Python submodule).

## Design Dependencies

- `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` (design `py-query-segment-registration`): whichever lands
  second puts `QuerySegment` in `liquers.query`.
- `py-default-environment-config` (PR #273, Phase 1): its `configure` goes in
  `liquers.environment`. `liquers-py-modules` (PR #265): its `import_module` goes there too.
- No published wheel exists yet (`PY-NO-PUBLISHED-WHEELS`); landing before the first release means
  the first published `__module__` names are already the submodules.

## Scope Changes

2026-11-14: started as a compact design (complexity `M`); converted to the full form at Phase 2,
because the module map and the stub and test changes do not fit the compact five pages
(complexity `L`).

## Decisions

Orest, 2026-11-14, at the Phase 1 gate:

- **D1 (Q1)** Classes report their submodule (`liquers.query.Key`). Accepted.
- **D2 (Q2, Q6)** Modules take the Rust names, except `liquers.lui` (Liquers UI) for `ui` and
  `liquers.data_nodes` for `data_node`, so the top-level functions `lq.ui()` and `lq.data_node()`
  keep working and live in those modules too. Orest rejected `ui_model`, and rejected turning
  `lq.ui` / `lq.data_node` into modules. Rejected too: a callable module object (module
  `__class__` swap), which type checkers cannot describe.
- **D3 (Q3)** `expiration` folds into `metadata`. Dependencies get their own module, which the
  future manager wrapper joins.
- **D4** The native extension is the private top-level module `_liquers`; `liquers` holds only the
  public modules and Python helpers. A pure-Python fallback must fit later: every public module
  reaches native names only through `liquers/_backend.py`.
  Phase 2 (R1) found that maturin needs `_liquers` to be a private top-level *package* holding the
  compiled `_liquers._liquers`; the decision stands, in that form.
- **D5 (Q4)** Every top-level `liquers.X` stays permanently as a re-export, with no deprecation;
  `lq.X` (`import liquers as lq`) stays the documented style.

Phase 1 approved by Orest ("proceed") on 2026-11-14.

## Open Questions

None. Q5 (pdoc) and Q7 (maturin layout) were implementation details, settled in Phase 2 by
experiment (Phase 2 *Overview*, R1 and R2).
