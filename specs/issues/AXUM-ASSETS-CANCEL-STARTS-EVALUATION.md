---
id: AXUM-ASSETS-CANCEL-STARTS-EVALUATION
kind: issue
title: POST /api/assets/cancel starts an evaluation before cancelling it
status: closed
priority: P3
complexity: S
area: [axum]
design: axum-assets-endpoints
created: 2026-09-27
github:
---
## Problem

`cancel_handler` (`liquers-axum/src/assets/handlers.rs`) obtains the asset with
`AssetManager::get_asset`, which for a key not yet evaluated creates the asset and submits it to
the job queue (or fast-tracks it from the store), and only then calls `cancel()`. Cancelling an
asset nobody asked for therefore starts its evaluation. It also answers 200 for a key that has no
value and no recipe.

## Impact

Wasted work, and a cancel is not the read-only operation `read_only()` routers assume when they
keep it routed. Low severity: the evaluation is cancelled immediately in most cases.

## Expected behaviour

Cancel only an asset the manager already holds (`lookup_key_asset` / the query map) and report
"nothing to cancel" otherwise, without evaluating.

## Discovery

Final cross-phase review of `specs/design/axum-assets-endpoints/`, 2026-09-27 (AAE53 posts
`cancel` on a key that does not exist).

## Resolution

`q/cancel` and `key/cancel` look the asset up without creating one (`lookup_query_asset`,
`lookup_key_asset`) and answer 404 `NotAvailable` when nothing live holds it, so cancelling never
starts an evaluation. Evidence: AAE05, AAE38, AAE39, AAE92. (A command already running is still not
interrupted: `ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY`.)

Fixed on branch `claude/fervent-cori-ew4kvn` (design `axum-assets-endpoints`, 2026-09-28).
