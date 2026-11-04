---
id: PIPELINES-NOT-IN-LANGUAGE-BINDINGS
kind: feature
title: Python and JavaScript cannot pass, evaluate or register pipelines
status: draft
priority: P2
complexity: L
area: [py, web]
design: liquers-py-pipelines
created: 2026-11-03
github:
---

## Problem

`design/composite-command-definition/` adds pipelines: a structured, JSON-representable form of a
query, used for pipeline commands, recipe pipelines and `Value::Pipeline` (AC-39). Its
documentation plan specifies, in `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Pipelines, what a
binding must support. Neither `liquers-py` (`liquers-py/src/`) nor `liquers-web`
(`liquers-web/src/`) implements any of it:

- passing a pipeline as a native object (a Python `dict` / `list`, a JavaScript object) and
  receiving one back;
- `Value::Pipeline` in the value bridge, both ways;
- evaluating a pipeline as an environment evaluates a query;
- registering a pipeline command from a native object;
- `PipelineEditor` and `QueryEditor` (the editors' part is `QUERY-BUILDER-NOT-IN-BINDINGS`, design
  `query-builder-in-bindings`, which moves from `QueryBuilder` to `QueryEditor`).

## Impact

Python and JavaScript users can use pipelines only as YAML or JSON files read by the core, for
example in `recipes.yaml`, and cannot build one in code. Workaround: build query text, which cannot
carry structured arguments or placeholders.

## Expected behaviour

Each binding implements the guide's §Pipelines and records it in its conformance matrix
(`liquers-py/README.md`, `liquers-web/README.md`). Pipelines cross the boundary as their JSON form,
the way plans do, so no binding needs its own pipeline types. Whether the editors are thin wrappers
or are reimplemented natively over the JSON form is for the bindings design to decide.

A design is needed (`L`). It follows `composite-command-definition` and can share
`query-builder-in-bindings`.

## Discovery

The maintainer's review of `composite-command-definition` on 2026-11-03 ("the language integration
guide should specify the pipeline support").
