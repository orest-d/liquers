---
id: CANCEL-CAN-OVERTAKE-A-RETURNED-COMMAND
kind: issue
title: A cancel can still win after the last command returned Ok, if the rest of the evaluation then suspends
status: draft
priority: P3
complexity: M
area: [core/assets]
design:
created: 2026-10-09
github:
---

## Problem

`asset-cancellation-outcome` D2 says that once the command returns `Ok`, finalizing and storing are
not cancellable. The run (`liquers-core/src/assets.rs` `AssetRef::run_with_future` /
`run_with_future_inline`) implements this by racing the whole compute future
(`AssetRef::compute` → `evaluate_recipe_outcome` → `apply_recipe`) against the cancellation request,
with compute polled first, and by doing finalization (`complete_evaluation`) outside the race.

The compute future does not end where the last command returns: the interpreter still runs a few
awaits after it (metadata updates through the asset's `RwLock`, `take_pending_dependencies`). They
are normally uncontended and complete in the same poll, so the completed result wins. But if one of
them suspends — the lock is held by the service loop writing a log line, or Tokio's cooperative
budget forces a yield — and a cancel was requested meanwhile, the race picks the request and a
finished computation is discarded as `Cancelled`.

## Impact

Rare, and the outcome is still consistent (one terminal status, nothing stored). But it breaks the
"a completed run wins" guarantee that `specs/guides/COMMAND_DESIGN_GUIDE.md` documents, for exactly
the long-running commands where losing the result is most costly. No workaround for the caller.

## Expected behaviour

Once the plan's last command has returned `Ok`, a cancel request no longer drops the evaluation.
Possible approaches (not chosen here): mark the point of no return on the shared
`CancellationRequest` from the interpreter after the final step, and have the run stop selecting on
the request once it is set; or split `apply_recipe` so the post-command bookkeeping runs outside the
race.

## Discovery

Found while implementing `design/asset-cancellation-outcome/` (Phase 3 learning log): reasoning
about which awaits remain in the compute future after a synchronous command returns. Not reproduced
by a test; the race window is a few uncontended awaits.
