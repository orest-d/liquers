---
id: CONTEXT-APPLY-AD-HOC-SKIPS-RELATIVE-PIPELINE-KEYS
kind: issue
title: Context::apply_ad_hoc refuses a relative query but not a relative key inside a recipe's pipeline
status: draft
priority: P3
complexity: S
area: [core/context]
design:
created: 2026-11-04
github:
---

## Problem

`Context::apply_ad_hoc` (`liquers-core/src/context.rs`, around line 1031) checks the recipe's
`query` for relative keys (`reject_relative_query`) and logs `"apply <query>"`. A recipe whose
`pipeline` holds the work and whose `query` is empty passes the check whatever its pipeline
contains, and is logged as `"apply "`.

**Example.** Inside a command, `context.apply_ad_hoc(Recipe { pipeline: Some(p), ..Default::default() }, State::new())`
with `p = ["data/x.csv", …]` (a relative resource key) is not refused as `"data/x.csv/-/…"` would
be; the key resolves against no working directory. Expected: the same refusal as the query form,
and a log line naming the pipeline.

## Impact

A binding that evaluates pipelines through `apply_ad_hoc` (`liquers-py-pipelines`, Decision 3) has to
repeat the check itself. Workaround: the binding walks the pipeline's operands first.

## Expected behaviour

`apply_ad_hoc` checks the recipe's combined pipeline (`Recipe::combined_pipeline`) for relative
operands with the same error, and its log line shows the pipeline when there is no query.

## Discovery

Found on 2026-11-04 in the review of `liquers-py-pipelines` Phase 2.
