---
id: QUERY-BUILDER-NOT-IN-BINDINGS
kind: feature
title: JavaScript has no validating query builder
status: draft
priority: P3
complexity: M
area: [web]
design: query-builder-in-bindings
created: 2026-10-21
github:
---

## Problem

`liquers-core` has `QueryEditor` and `PipelineEditor` (`liquers-core/src/pipeline/editor.rs`),
which check every edit against the command registry. `liquers-web` exposes neither: a JavaScript
caller escapes one parameter at a time (`encodeParam`, `liquers-web/src/encode.rs`) and assembles
resource segments, the `-/` header, action names and filenames as text, with no validation.

**Example.** `"-R/" + folder + "/" + name + "/-/ns-rec/eq-region-" + value` with `name =
"my file.csv"` gives text that does not parse, and a forgotten `-/` turns the actions into key
segments silently. Expected: an editor whose steps are checked, and a refused step throwing a
`LiquersError` of type `parameter_error` naming the token.

## Impact

JavaScript code that builds queries keeps its own string assembly, the drift the core editors
exist to remove. Workaround: careful text assembly.

## Expected behaviour

A JavaScript wrapper over `QueryEditor` / `PipelineEditor` with no escaping or validation logic of
its own (design `query-builder-in-bindings`, to be re-based; the Python precedent is
`liquers-py-pipelines` question 2).

## Discovery

Split off on 2026-10-21 by the bulk design of `QUERY-BUILDER-TOOLING`. Narrowed to JavaScript on
2026-11-04: the core `QueryBuilder` it first named was replaced by `QueryEditor` /
`PipelineEditor` (`composite-command-definition`), and the Python half is
`PY-HAS-NO-VALIDATING-QUERY-EDITOR`, implemented by `liquers-py-pipelines` (Orest at its Phase 1
gate: "agreed").
