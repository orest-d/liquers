---
id: PIPELINE-RESULTS-ARE-NEVER-CACHED
kind: feature
title: A pipeline's result is never cached, even when the pipeline has a query form
status: draft
priority: P2
complexity: L
area: [core/assets, core/context]
design: 
created: 2026-11-14
github:
---
## Problem

An asset is identified by its query, and a pipeline has no such identity. Evaluating a pipeline
(`interpreter::evaluate_pipeline`, Python `env.evaluate(pipeline)`, JavaScript
`evaluatePipeline`) therefore runs its steps on every call, and nothing can ask for "the asset
of a pipeline" (Python `env.get_asset(pipeline)` raises `TypeError`).

This also holds for a pipeline that *has* a query form, one that `to_query_with` converts
without loss, although such a pipeline means exactly what its query means and that query's
asset may already be cached.

The bindings do not behave alike in detail. Probed on 2026-11-14 with counting commands,
evaluating `["numbers", {"action": "top", "arguments": {"n": 2}}]` twice:

- Python: `numbers` ran once and `top` twice. The leading query text is evaluated as a query,
  so it is cached; the action after it is not.
- Browser module: `numbers` and `top` both ran twice.

## Impact

A program that receives its computations as pipelines (the case pipelines exist for: structured
arguments, JSON from another program) loses caching, deduplication of concurrent requests,
dependency tracking and refresh. The user guides now say so (`docs/guides/_shared/pipeline-or-query.md`)
and advise a query wherever one can say the same.

Workarounds: a recipe with `pipeline:` gives the result a key (`reference/RECIPES_AND_PLANS.md`),
and a pipeline registered as a command is called by an ordinary, cached query.

## Expected behaviour

In steps of growing cost:

1. A pipeline with a query form is evaluated as that query, so it shares the query's asset.
2. The bindings agree on what a leading query-text element caches.
3. A pipeline without a query form gets an identity of its own (for example a hash of its
   canonical JSON) under which its result can be cached and asked for as an asset. This is a
   design decision: the asset manager, metadata and the web API all name an asset by a query.

## Discovery

Writing the pipeline sections of the three user guides (chapter 3.10), 2026-11-14.
