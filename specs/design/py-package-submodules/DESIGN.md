---
id: PY-PACKAGE-SUBMODULES
kind: design
title: Public submodules for the liquers Python package, mirroring the Rust modules
form: compact
workflow: liquers-project
status: in_review
phase: high-level
area: [py]
issues: [PY-API-IS-ONE-FLAT-MODULE]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, docs/guides/python/01-introduction.md, docs/guides/python/04-values.md]
created: 2026-11-14
---
# Public submodules for the liquers Python package

## Phase 1: High-Level Design

### Purpose

The `liquers` package puts about 120 public names in one namespace (89 listed in `__init__.py`
plus the generated error classes), so a reader who knows the Rust crates' modules cannot use that
structure in Python. This design moves the native extension to a private top-level module
`_liquers` and gives `liquers` public submodules named after the Rust modules, each a thin Python
module over `_liquers`, so a pure-Python fallback can later replace `_liquers` behind them.

Requested by Orest on 2026-11-14; Phase 1 gate answers of the same day are recorded under
*Decisions*.

### Problem Example

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

Module map. Each module takes the Rust module's name; where a top-level function had that name
(`ui`, `data_node`), the function moves into the module and `lq.ui` / `lq.data_node` become the
modules (*Decisions* D2).

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
| `liquers.data_node` | `data_node`, `DataNode`, `NodeKind` | `liquers_core::data_node` |
| `liquers.pipeline` | `Pipeline`, `PipelineEditor`, `QueryEditor`, `ActionEditor` | `liquers_core::pipeline` |
| `liquers.template` | `RenderResult`, `Diagnostic` | `liquers-template`, `liquers_lib::template` |
| `liquers.records` | `RecordView`, `RecordSource`, `RecordStream`, `RecordChunk`, `RecordColumn`, `RecordBuffer`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin` | `liquers-records` |
| `liquers.server` | `serve`, `serve_async`, `Server` | `liquers-axum` |
| `liquers.ui` | `ui`, `UI`, `UIElement`, `UIElementInfo`, `UIStatusEvent`, `UISubscription` | `liquers_lib::ui` |

### Scope and Acceptance Criteria

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
  WHEN a program uses a name of today's `liquers.__all__` other than `ui` and `data_node` as
  `liquers.X`
  THEN it gets the same object as `liquers.<submodule>.X`
- **AC-3** Functions named like their module live in it
  WHEN `import liquers as lq` and then `lq.ui.ui()` and `lq.data_node.data_node({"a": 1})` run, and
  `import liquers.ui as m` runs in either order with `import liquers`
  THEN `lq.ui` and `m` are the module `liquers.ui`, and both calls return what `lq.ui()` and
  `lq.data_node(...)` returned before
- **AC-4** Classes report their submodule
  WHEN `type(lq.parse("a/b")).__module__` or `lq.KeyNotFoundError.__module__` is read
  THEN it is `"liquers.query"` or `"liquers.error"`: every public class reports the submodule that
  exports it
- **AC-5** One home per name
  WHEN the `__all__` lists of all public submodules are collected
  THEN each public name appears in exactly one of them, and their union plus the submodule names
  equals `liquers.__all__`
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

### Documentation Intent

- `specs/reference/PYTHON_INTEGRATION.md` §1 Layout (`_liquers`, the seam, the public submodules),
  the `__module__` rule, §13 the stub layout, §14 and §18 for `data_node` and `ui`.
- `specs/guides/PYTHON_GUIDE.md`: a "Package layout" section mapping submodules to Rust modules.
- `docs/guides/python/01-introduction.md`: the package layout; `04-values.md`: the
  `data_node` calls (the guide has no `lq.ui()` call). Examples keep `import liquers as lq`.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: the lesson "PyO3's `module = "liquers"` decides
  where the stubs go", updated to submodules.
- `liquers-py/README.md`: the OBJECT08 row.
- API docs: `liquers-py/python/liquers/__init__.py` (the package doc gains the submodule map), a
  module doc for each of the 16 public submodules and for `_backend.py`; `liquers-py/src/lib.rs`
  `//!` (module `_liquers`, classes report their Python submodule).

### Design Dependencies

- `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` (design `py-query-segment-registration`): whichever lands
  second puts `QuerySegment` in `liquers.query`.
- `py-default-environment-config` (PR #273, Phase 1): its `configure` goes in
  `liquers.environment`. `liquers-py-modules` (PR #265): its `import_module` goes there too.
- No published wheel exists yet (`PY-NO-PUBLISHED-WHEELS`), so the D2 break reaches no released
  user; it must land before the first release.

### Decisions

Orest, 2026-11-14, at the Phase 1 gate:

- **D1 (Q1)** Classes report their submodule (`liquers.query.Key`). Accepted.
- **D2 (Q2)** Modules take the Rust names. A same-named function lives in its module and calls
  `_liquers` (`liquers.ui.ui`), rather than `ui_model` / `data_nodes`, which Orest rejected as
  unclear and one letter from a typo. Proposed consequence, pending confirmation: `lq.ui` and
  `lq.data_node` become the modules; callers write `lq.ui.ui()` or `env.ui()`, and
  `from liquers.data_node import data_node`. Rejected: a callable module object (module
  `__class__` swap), which type checkers cannot describe.
- **D3 (Q3)** `expiration` folds into `metadata`. Dependencies get their own module, which the
  future manager wrapper joins.
- **D4** The native extension is the private top-level module `_liquers`; `liquers` holds only the
  public modules and Python helpers. A pure-Python fallback must fit later: every public module
  reaches native names only through `liquers/_backend.py`.

### Open Questions

- **Q4 (open design; recommended answer given) — top-level names.** Keep every other `liquers.X`
  permanently as a re-export, with no deprecation, and keep `lq.X` as the documented style.
- **Q6 (open design; recommended answer given) — the D2 consequence.** Accept that `lq.ui()` and
  `lq.data_node(x)` stop working (2 and 60 call sites, all in this repository's tests and guides),
  before the first release.
- **Q5 (implementation detail, Phase 2)** — pdoc's rendering of re-exported names on the
  `liquers` page, and DOCS04 accordingly.
- **Q7 (implementation detail, Phase 2)** — maturin's layout for a top-level extension beside a
  Python package (`module-name = "_liquers"` with `python-packages = ["liquers"]`), checked by
  building a wheel; and where stubs of a top-level private module go (PEP 561 covers packages, not
  single modules).
