---
id: PY-API-REFERENCE-NOT-GENERATED
kind: feature
title: The Python package has no generated API reference, and most of its names have no docstring
status: draft
priority: P2
complexity: M
area: [py, docs]
design:
created: 2026-10-19
github:
---

## Problem

`liquers` has no standard Python API reference: nothing generates one, and there is little to
generate it from. On 2026-10-19, 285 of the 392 public names (`__all__` plus each class's public
attributes) had no docstring, among them `Position`, `Query`, `Key` and every method of `Query`,
`Metadata` and `MetadataRecord`. The classes are PyO3 wrappers whose methods have no `///`
comment and no `text_signature`, so `help(lq.Query)` lists bare names.

The two hand-written documents cover how to use the package and how it is built
(`guides/PYTHON_GUIDE.md`, `reference/PYTHON_INTEGRATION.md`); neither is, or should become, a
per-method reference.

## Problem Example

```python
help(lq.Query)        # lists method names with no description and no signature
lq.Query.encode.__doc__   # None
```

Re-checked at HEAD (2026-10-26): no `pdoc`/Sphinx configuration or build script exists
(`scripts/` has only `test-liquers-py.sh`), and no CI job runs the Python tests
(`LANGUAGE-BINDING-TESTS-NOT-RUN-IN-CI`).

## Impact

A Python user, an IDE or a coding agent has no per-class, per-method description of the API, its
signatures or its return types. `help()` and editor tooltips say nothing for most names.

## Expected behaviour

1. Every public class, method, property and function of `liquers` has a docstring: a `///`
   comment on each `#[pymethods]` item (PyO3 turns it into `__doc__`), with a `text_signature`
   where PyO3 cannot infer one.
2. A script, for example `scripts/build-liquers-py-docs.sh`, builds the package and generates an
   HTML API reference with a standard tool (`pdoc` is the smallest choice; Sphinx autodoc if the
   reference is to join other Python docs). `scripts/test-liquers-py.sh` runs it, so a broken
   docstring fails the test loop.
3. A test fails when a public name has no docstring, so coverage cannot regress.

The method-versus-property inconsistency of `Metadata` is `PY-METADATA-FIELDS-ARE-METHODS`.

Type stubs (`.pyi`, `py.typed`) are the guide's separate `STUBS` feature and are not required here,
though a generator that reads the same docstrings would serve both.

## Discovery

Requested by the owner when closing `design/liquers-py/` (2026-10-19): a standard Python API doc,
a Python reference and a Python guide. The guide and the reference were written in that design's
Phase 5; this is the remaining part. Tracked in `reference/api/API_DOCS_GAP_ANALYSIS.md` as part
of DOC-12, and an instance of `LANGUAGE-GUIDE-NO-DOCUMENTATION-SECTION`.
