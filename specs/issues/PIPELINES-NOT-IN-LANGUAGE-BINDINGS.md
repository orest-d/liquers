---
id: PIPELINES-NOT-IN-LANGUAGE-BINDINGS
kind: feature
title: Python cannot pass, evaluate or register pipelines
status: in_progress
priority: P2
complexity: L
area: [py]
design: liquers-py-pipelines
created: 2026-11-03
github:
---

## Problem

`design/composite-command-definition/` adds pipelines: a structured, JSON-representable form of a
query, used for pipeline commands, recipe pipelines and `Value::Pipeline` (AC-39). Its
documentation plan specifies, in `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §Pipelines, what a
binding must support. `liquers-py` (`liquers-py/src/`) implements none of it, beyond returning a
pipeline value and `Recipe.pipeline` as a `dict`:

- passing a pipeline as a native object (a Python `dict` / `list`) and
  receiving one back;
- `Value::Pipeline` in the value bridge, both ways;
- evaluating a pipeline as an environment evaluates a query;
- registering a pipeline command from a native object;
- `PipelineEditor` and `QueryEditor` (the editors' part is `QUERY-BUILDER-NOT-IN-BINDINGS`, design
  `query-builder-in-bindings`, which moves from `QueryBuilder` to `QueryEditor`).

## Impact

Python users can use pipelines only as YAML or JSON files read by the core, for
example in `recipes.yaml`, and cannot build one in code. Workaround: build query text, which cannot
carry structured arguments or placeholders.

## Expected behaviour

`liquers-py` implements the guide's §PIPELINE and records it in its conformance matrix
(`liquers-py/README.md`). Pipelines cross the boundary as their JSON form, so no rule is
reimplemented in Python; whether Python also gets a wrapper class is the design's question 1.
Design: `liquers-py-pipelines`.

## Discovery

The maintainer's review of `composite-command-definition` on 2026-11-03 ("the language integration
guide should specify the pipeline support").

## Progress

2026-11-04: the `liquers-web` half is done by design `web-pipelines` (PR #256): pipelines as plain
objects, `evaluatePipeline`, the two conversions, wrapped editors, and pipeline commands through
`registerCommands`. `liquers-py` remains; the web design's answers to the guide's four questions
(native structure, wrapped editors, `ParameterError`, JSON-only placeholder values) are a
reasonable default for it.

**Scope split, 2026-11-04.** Narrowed to Python by `liquers-py-pipelines` (Phase 1). The
JavaScript half is `WEB-CANNOT-USE-PIPELINES`.
