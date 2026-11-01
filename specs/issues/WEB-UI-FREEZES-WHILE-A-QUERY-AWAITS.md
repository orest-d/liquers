---
id: WEB-UI-FREEZES-WHILE-A-QUERY-AWAITS
kind: issue
title: The browser UI stops updating while any of its queries awaits a Promise
status: draft
priority: P2
complexity: L
area: [lib/ui, core/assets, web]
design:
created: 2026-10-30
github:
---
## Problem

On wasm the environment uses `ImmediateAssetManager` (`liquers-core/src/assets.rs`), which runs
every evaluation in the caller's task. `mount_web`'s driver loop (`liquers-lib/src/ui/web/app.rs`)
awaits `AppRunner::run`, and `run` awaits each evaluation it starts: `evaluate_immediately` for
`SubmitQuery`, `evaluate` for pending nodes and `RequestAssetUpdates`, `apply` for `ApplyToInput`
(`liquers-lib/src/ui/runner.rs`). When a query reaches an async JavaScript command that awaits
`fetch`, the whole loop waits for it. No message is processed and no invalidation is applied, so
the progress element `evaluate_pending_nodes` sets is never painted. The page only sees the final
result.

## Impact

A browser UI built on liquers-web (`web-ui-use`) shows no progress for slow queries, and it ignores
clicks and other UI messages until the slow query finishes. Status events still arrive in order, so
the problem is the display, not correctness. Workaround: keep slow work out of queries a UI
evaluates.

## Expected behaviour

The runner starts an evaluation and goes on. It finishes the evaluation later, for example by
spawning the future with `spawn_ui_task` and collecting its result through a channel that Phase 3
of `run` polls. Each element keeps its stale-result rule: only the newest evaluation per handle
lands. This needs a design, since it changes when `run` returns and what `needs_repaint` means.

## Discovery

Found while writing Phase 2 of `specs/design/web-ui-use/` (2026-10-30): reading `runner.rs` against
`ImmediateAssetManager`'s doc comment ("Every evaluation runs in the caller's task").
