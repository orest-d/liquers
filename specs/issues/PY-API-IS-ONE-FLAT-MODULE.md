---
id: PY-API-IS-ONE-FLAT-MODULE
kind: feature
title: The Python API is one flat module with no submodules, unlike the Rust crates
status: draft
priority: P3
complexity: M
area: [py]
design: py-package-submodules
created: 2026-11-14
github:
---
## Problem

The `liquers` package exposes about 90 public names from one namespace. `liquers-py/src/lib.rs`
registers every class and function on the single native module `liquers._native`,
`liquers-py/python/liquers/__init__.py` re-exports all of them, every `#[pyclass]` says
`module = "liquers"`, and the stub `liquers-py/python/liquers/__init__.pyi` (about 1,700 lines)
declares them in one file. The pdoc reference built by `scripts/build-liquers-py-docs.sh` is one
page. The Rust side has a module per concept (`liquers_core::query`, `command_metadata`,
`metadata`, `plan`, `recipes`, `store`, `context`, `assets`, `error`; `liquers-records`;
`liquers-template`; the `liquers-lib` UI; `liquers-axum`), so a reader cannot use the Rust
structure to find their way in the Python API.

## Impact

Orientation and discoverability only: every name works, but the reference and autocompletion
list query classes, record classes, UI classes and error classes side by side. No workaround
beyond the comment groups in `__init__.py`.

## Expected behaviour

Public submodules roughly mirroring the Rust modules (`liquers.query`, `liquers.commands`,
`liquers.metadata`, `liquers.plan`, `liquers.recipes`, `liquers.store`, `liquers.environment`,
`liquers.assets`, `liquers.errors`, `liquers.records`, `liquers.templates`, `liquers.pipelines`,
`liquers.data_nodes`, `liquers.ui_model` or similar, `liquers.server`), each a thin Python module
re-exporting from `liquers._native`, with each class's `__module__` naming its submodule. Every
top-level `liquers.X` name stays as the same object, permanently. Two submodule names must not
reuse a top-level function name (`liquers.ui()` and `liquers.data_node()` exist), because
importing a submodule rebinds that attribute of the package.

## Discovery

Raised by Orest on 2026-11-14 ("python code is organised in a single module, which makes it
difficult to orientate in the API"); the assessment is in the project thread *Split Python API
into submodules*.
