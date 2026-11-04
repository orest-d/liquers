---
id: WEB-PAGE-CANNOT-PASS-A-PIPELINE-VALUE
kind: feature
title: A page cannot pass a pipeline to a command as a value
status: draft
priority: P3
complexity: S
area: [web]
design: 
created: 2026-11-04
github:
---
## Problem

`Value::Pipeline` (`liquers-core/src/value.rs`) crosses to JavaScript as its JSON object
(`liquers-web/src/bridge.rs` `value_to_js`), but nothing crosses back as one: `js_to_value` turns
a page's object into a map value. A page therefore cannot hand a command a pipeline *value* (a
command argument or a returned value that a later step uses as a pipeline). Pipelines as
*definitions* and *evaluation inputs* do work (`liquers-web/src/pipeline.rs`, design
`web-pipelines`).

## Impact

Low today: no built-in command takes a pipeline value, and a page evaluates or registers a pipeline
through `evaluatePipeline` / `registerCommands`. It matters once a command consumes
`Value::Pipeline`. `liquers-py` has the same asymmetry (a pipeline returns as a dict).

## Expected behaviour

An explicit opt-in, in the way `liquers.opaque(x)` is one: for example `liquers.pipeline(obj)`
returning a `Value` that holds `Value::Pipeline`, refused when the object is not a valid pipeline.
Structural conversion of a plain object must stay a map; guessing would change existing commands'
inputs.

## Discovery

Implementing design `web-pipelines` on 2026-11-04; recorded in its Phase 5 and in
`LANGUAGE-GUIDE-PIPELINE-COVERAGE-INCOMPLETE`.
