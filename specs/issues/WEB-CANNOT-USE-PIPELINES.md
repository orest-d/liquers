---
id: WEB-CANNOT-USE-PIPELINES
kind: feature
title: JavaScript cannot pass, evaluate or register pipelines
status: closed
priority: P2
complexity: L
area: [web]
design: web-pipelines
created: 2026-11-04
github:
---

## Problem

`liquers-web` (`liquers-web/src/`) implements none of the guide's §PIPELINE
(`specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`): passing a pipeline as a JavaScript object and
receiving one back, `Value::Pipeline` in the value bridge both ways, evaluating a pipeline,
registering a pipeline command, and `PipelineEditor` / `QueryEditor` (the editors' part is
`QUERY-BUILDER-NOT-IN-BINDINGS`).

**Example.** `env.evaluate([{"action": "hello"}])` in JavaScript throws, because `evaluate`
takes a string (`liquers-web/src/environment.rs`). Expected: the pipeline is evaluated as the query `hello` (guide test `PIPELINE01`).

## Impact

JavaScript users can use pipelines only as files read by the core. Workaround: build query text,
which cannot carry structured arguments or placeholders.

## Expected behaviour

`liquers-web` implements §PIPELINE and records it in `liquers-web/README.md`. The Python design
`liquers-py-pipelines` is the precedent for the class, editor and entry-point choices. A design is
needed (`L`).

## Discovery

Split from `PIPELINES-NOT-IN-LANGUAGE-BINDINGS` on 2026-11-04, when `liquers-py-pipelines` took
its Python half.

## Resolution

2026-11-04: done by design `web-pipelines` (PR #256), which was written against
`PIPELINES-NOT-IN-LANGUAGE-BINDINGS` before that issue was split; it now points here.
