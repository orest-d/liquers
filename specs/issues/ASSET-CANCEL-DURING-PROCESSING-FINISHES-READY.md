---
id: ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY
kind: issue
title: An asset cancelled while its command runs is finalized Ready when the command returns
status: accepted
priority: P2
complexity: L
area: [core/assets]
design: asset-cancellation-outcome
created: 2026-09-28
github:
---
## Problem

`AssetRef::cancel` (`liquers-core/src/assets.rs`, ≈3125) sets the asset's cancelled flag and sends
`AssetServiceMessage::Cancel`, then waits up to 5 s for `JobFinished`. When the asset is
`Processing` a command that is already running is not interrupted, which is expected. But when the
command returns, the completion path (`finalize_status_with_version`, ≈2870) finalizes the asset as
`Ready` regardless of the flag; the flag only suppresses the store write (`save_to_store`). The
queued `Cancel` message is never acted on, because the service loop that would set `Cancelled` has
already finished.

So a client that cancels a running evaluation sees the asset become `Ready`, with a value that was
never persisted, and `cancel()` returns `Ok(())`.

## Reproduction

`liquers-axum/tests/assets_api_endpoints.rs`, `aae92_cancel_while_processing_reports_cancelled_deterministically`:
a command that blocks for 500 ms is submitted with `POST q/submit`, polled until `Processing`, then
cancelled with `POST q/cancel`; `q/info` then reports `Ready`, never `Cancelled`. `aae38` shows the
same through `GET q/cancel`. Both tests currently accept either terminal status and cite this issue.

## Expected behaviour

An asset whose cancellation was requested before its evaluation finished ends `Cancelled` (its value
dropped, `JobFinished` sent), or the API documents that cancelling a running command is
best-effort and reports whether it took effect. The web API's `q/cancel` and `key/cancel` return
the asset's `AssetInfo` after the cancel, so either answer is visible to the client; the silent
`Ready` is the problem.

## Discovery

Found 2026-09-28 while implementing `specs/design/axum-assets-endpoints/` (Step 12): Phase 3's
cancel tests assumed a cancelled `Processing` query ends `Cancelled`.
