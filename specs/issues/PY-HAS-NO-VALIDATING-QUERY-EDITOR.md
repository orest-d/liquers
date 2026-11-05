---
id: PY-HAS-NO-VALIDATING-QUERY-EDITOR
kind: feature
title: Python has no validating query or pipeline editor
status: draft
priority: P3
complexity: M
area: [py]
design: liquers-py-pipelines
created: 2026-11-04
github:
---

## Problem

`liquers-core` has `QueryEditor` and `PipelineEditor` (`liquers-core/src/pipeline/editor.rs`),
which check every edit against the command registry and refuse it without changing anything.
`liquers-py` exposes neither, so a Python caller assembles query text and escapes parameters one at
a time (`ActionRequest.from_arguments`).

**Example.** `"-R/" + folder + "/" + name + "/-/ns-pl/is_in-region-EU"` with `name = "my file.csv"`
gives text that does not parse, and a forgotten `-/` silently turns actions into key segments.
Expected: `env.query_editor().resource(...)` and `.action("is_in", ns="pl").set(...)`, each step
checked, a refused step raising `lq.ParameterError` (guide test `PIPELINE05`).

## Impact

Python code that builds queries keeps its own string assembly. Workaround: careful text assembly.

## Expected behaviour

Wrapper classes over the core editors, with no escaping or validation logic of their own
(`liquers-py-pipelines` question 2).

## Discovery

Split on 2026-11-04 from `QUERY-BUILDER-NOT-IN-BINDINGS`, whose core `QueryBuilder` no longer exists,
when Orest approved `liquers-py-pipelines` question 6.
