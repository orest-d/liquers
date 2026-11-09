---
id: TOP-LEVEL-RELATIVE-RESOURCE-QUERY-LOOKS-UP-A-RELATIVE-RECIPES-KEY
kind: issue
title: A top-level query reading a relative key fails on './recipes.yaml', while its pipeline form resolves the key
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-11-04
github:
---

## Problem

At the top level, with no working directory set, `env.evaluate("-R/./x.txt")` fails with
`KeyNotAbsoluteError: Key './recipes.yaml' is not absolute`: looking for a recipe for the key, the
asset manager asks the store for the recipes file beside the *unresolved* key. The same read as a
pipeline, `[{"resource": "./x.txt"}]` evaluated through a pipeline-only recipe (`apply_queued`),
logs `Relative key/query has no CWD; using logical root '/'` and returns the root's `x.txt`.

With a working directory (`-R-cwd/data/-R/./x.txt`, or `[{"cwd": "data"}, {"resource": "./x.txt"}]`)
both forms agree.

## Impact

A query and the pipeline it converts to behave differently for one input, and the query's error
names a file the user never asked for. Found while writing
`liquers-py/tests/test_PIPELINE_evaluate.py::test_relative_key_pipeline`, which tests the agreeing
case only. Expected: either both refuse a relative key with no working directory (as a command's
context does), or both resolve it against the root.
