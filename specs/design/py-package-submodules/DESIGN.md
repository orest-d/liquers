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
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, docs/guides/python/01-introduction.md]
created: 2026-11-14
---
# Public submodules for the liquers Python package

## Phase 1: High-Level Design

### Purpose

The `liquers` package puts about 120 public names in one namespace (89 listed in `__init__.py` plus the generated error classes): `liquers-py/src/lib.rs`
`_native` registers every class on the one native module, `liquers-py/python/liquers/__init__.py`
re-exports them all, every `#[pyclass]` says `module = "liquers"`, and one 1,700-line
`__init__.pyi` declares them. The Rust crates have a module per concept, so a reader who knows the
Rust API cannot use that structure to find their way in the Python one, and the pdoc reference
(`scripts/build-liquers-py-docs.sh`) is a single page. This design adds public submodules that
roughly mirror the Rust modules, without breaking any existing `liquers.X` name.

Requested by Orest on 2026-11-14; he approved starting the design after the cost assessment in the
project thread.

### Problem Example

```python
import liquers as lq
len(lq.__all__)                  # today: ~120 names: query, record, UI and error classes mixed
from liquers.query import Key    # today: ModuleNotFoundError: No module named 'liquers.query'
help(lq)                         # today: one alphabetical list of everything
```

What should happen: `from liquers.query import Key` works, `liquers.query.Key is lq.Key`,
`help(liquers.query)` shows the query classes with a module doc, the reference has one page per
submodule, and `lq.Key` keeps working unchanged.

Proposed module map (each submodule names its Rust counterpart in its module doc):

| Python module | Contents | Rust counterpart |
|---|---|---|
| `liquers.query` | `Query`, `Key`, `ActionRequest`, `ActionParameter`, `ResourceName`, `Position`, `SegmentHeader`, `TransformQuerySegment`, `ResourceQuerySegment`, `QuerySource`, `parse`, `parse_key`, `Link`, `link` | `liquers_core::query`, `parse` |
| `liquers.commands` | `command`, `first_command`, `CommandMetadata`, `CommandDefinition`, `CommandKey`, `CommandPreset`, `CommandMetadataRegistry`, `ArgumentInfo`, `ArgumentType`, `EnumArgument`, `EnumArgumentType` | `liquers_core::command_metadata`, `commands` |
| `liquers.metadata` | `Metadata`, `MetadataRecord`, `AssetInfo`, `Status`, `LogEntry`, `LogEntryKind`, `Version`, `DependencyKey`, `DependencyRecord`, `Expires`, `ExpirationTime` | `liquers_core::metadata`, `expiration` |
| `liquers.plan` | `Plan`, `Step`, `ParameterValue`, `ResolvedParameterValues`, `build_plan`, `DependencyRelation`, `PlanDependency` | `liquers_core::plan`, `dependencies` |
| `liquers.recipes` | `Recipe`, `RecipeList`, `RecipeProviders` | `liquers_core::recipes` |
| `liquers.store` | `Store`, `StoreArgumentInfo` | `liquers_core::store` |
| `liquers.environment` | `Environment`, `Context`, `Issue`, `default_environment`, `evaluate`, `evaluate_async`, `require` | `liquers_core::context`, `environment_builder` |
| `liquers.assets` | `State`, `AssetRef` | `liquers_core::state`, `assets` |
| `liquers.errors` | `Error`, `ErrorType`, `LiquersError` and its generated subclasses | `liquers_core::error` |
| `liquers.data_nodes` | `DataNode`, `NodeKind`, `data_node` | `liquers_core::data_node` |
| `liquers.pipelines` | `Pipeline`, `PipelineEditor`, `QueryEditor`, `ActionEditor` | `liquers_core::pipeline` |
| `liquers.templates` | `RenderResult`, `Diagnostic` | `liquers-template` |
| `liquers.records` | `RecordView`, `RecordSource`, `RecordStream`, `RecordChunk`, `RecordColumn`, `RecordBuffer`, `RecordSchema`, `FieldSchema`, `FieldType`, `KeyRole`, `ChunkOrigin` | `liquers-records` |
| `liquers.server` | `Server`, `serve`, `serve_async` | `liquers-axum` |
| `liquers.ui_model` | `UI`, `UIElement`, `UIElementInfo`, `UIStatusEvent`, `UISubscription`, `ui` | `liquers_lib::ui` |

Two names differ from Rust on purpose: a submodule `liquers.data_node` or `liquers.ui` would
collide with the existing top-level functions `lq.data_node()` and `lq.ui()`, because importing a
submodule binds it as an attribute of the package, and `import liquers.ui as m` then reads that
attribute (Q2).

### Scope and Acceptance Criteria

In scope: the public submodules above, each class's reported module, the type declarations, the
API reference and the documentation. Out of scope: renaming or removing any public name, changing
the native module `liquers._native` or the private Python modules (`_env`, `_commands`, `_errors`,
`_ui`, `_records`, `_factories`, `_extras`, which Rust code imports by name), and store-backed
module import (`liquers-py-modules`, PR #265).

- **AC-1** Import from a submodule
  WHEN `from liquers.query import Key, parse` runs in a fresh interpreter
  THEN both names import, and `liquers.query.Key is liquers.Key`
- **AC-2** Top-level names unchanged
  WHEN a program uses any name in the current `liquers.__all__` as `liquers.X`
  THEN it gets the same object as before, and the existing test suite passes without edits beyond
  the reported-module assertion of AC-3
- **AC-3** Classes report their submodule
  WHEN `type(lq.parse("a/b")).__module__` or `lq.KeyNotFoundError.__module__` is read
  THEN it is `"liquers.query"` or `"liquers.errors"`: every public class reports the submodule that
  exports it (Q1)
- **AC-4** One home per name
  WHEN the `__all__` lists of all public submodules are collected
  THEN each public name of `liquers.__all__` appears in exactly one of them, and their union equals
  `liquers.__all__`
- **AC-5** No collision with top-level functions
  WHEN `import liquers.ui_model` and `import liquers.data_nodes` run, in either order, before or
  after `import liquers`
  THEN `lq.ui` and `lq.data_node` are still the functions
- **AC-6** Type declarations follow the submodules
  WHEN stubtest and pyright run over the installed package (`tests/test_STUBS_declarations.py`)
  THEN they pass with one `.pyi` per public submodule, `liquers/__init__.pyi` re-exporting them, and
  `from liquers.records import RecordView` typed
- **AC-7** Reference has a page per submodule
  WHEN `scripts/build-liquers-py-docs.sh` runs
  THEN pdoc writes a page per public submodule, each with a module doc meeting
  `specs/guides/API_DOCUMENTATION_GUIDE.md` §3, and the `liquers` page still lists every
  `__all__` name
- **AC-8** Private modules undisturbed
  WHEN a Python command, a Python store, a records conversion or an async UI wait runs
  THEN the Rust imports of `liquers._ui`, `liquers._env`, `liquers._commands`, `liquers._errors`,
  `liquers._records` and `liquers._factories` still resolve

### Documentation Intent

- `specs/reference/PYTHON_INTEGRATION.md` §1 Layout: the public submodules and their contents; the
  `__module__` rule; §13 the stub layout.
- `specs/guides/PYTHON_GUIDE.md`: a "Package layout" section mapping submodules to Rust modules.
- `docs/guides/python/01-introduction.md`: a short paragraph and table on the package layout, so
  the user guide says where to look; examples keep the `import liquers as lq` / `lq.X` style.
- `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`: update the lesson "PyO3's `module = "liquers"`
  decides where the stubs go" to the submodule form.
- `liquers-py/README.md`: OBJECT08 row wording, if the reported module changes.
- API docs: `liquers-py/python/liquers/__init__.py` (the package doc gains the submodule map), and
  a new module doc for each of the 15 public submodules; `liquers-py/src/lib.rs` `//!` notes that
  classes report their Python submodule.

### Design Dependencies

- Overlap triage: `PY-QUERY-SEGMENT-CLASS-NOT-REGISTERED` (its own design
  `py-query-segment-registration`) touches the query exports; it stays separate, and whichever lands
  second puts `QuerySegment` in `liquers.query`. `liquers-py-modules` (PR #265, Phase 1) adds
  `lq.import_module` and `Environment` methods; it would place `import_module` in
  `liquers.environment`. No other open design changes the package layout.

### Open Questions

- **Q1 (open design; recommended answer given) — reported module.** Should classes report their submodule
  (`liquers.query.Key` in reprs, errors and the reference) or keep `liquers` (`liquers.Key`)?
  Recommended: the submodule. It is what makes the reference and IDE hovers show the structure, and
  stubtest requires the declarations to sit where `__module__` points, so keeping `liquers` would
  leave the 1,700-line stub in one file. Cost: messages show the longer name, and OBJECT08's
  assertion changes from `== "liquers"` to "a public submodule of `liquers`".
- **Q2 (open design; recommended answer given) — names of the UI and data-node modules.** Recommended: `liquers.ui_model`
  and `liquers.data_nodes` (plural like `commands`, `records`, `errors`), avoiding the clash with
  `lq.ui()` and `lq.data_node()`. Alternatives: rename the functions instead (breaking), or keep
  the Rust names and rely on `__init__` rebinding the functions (fragile: `import liquers.ui as m`
  returns the function).
- **Q3 (open design; recommended answer given) — granularity.** Recommended: the 15 modules above, merging small Rust
  modules (`expiration` into `metadata`, `dependencies` into `plan`). Alternative: a coarser split
  of about six (`query`, `commands`, `environment`, `errors`, `records`, `ui_model`).
- **Q4 (open design; recommended answer given) — top-level names.** Recommended: keep every `liquers.X` permanently, with
  no deprecation warning, and keep `lq.X` as the documented style (as polars and numpy do);
  submodules are the map and an alternative import path.
- **Q5 (implementation detail, Phase 2)** — whether pdoc renders each re-exported name on the `liquers` page
  as a full entry or as a link to its submodule page, and whether DOCS04 must change accordingly.
