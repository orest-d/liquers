---
id: METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH
kind: issue
title: A command that logs or reports progress panics under the immediate manager on native without a tokio runtime
status: draft
priority: P2
complexity: S
area: [core/assets]
design: inline-metadata-save-without-runtime
created: 2026-10-20
github:
---

## Problem

`ImmediateAssetManager` is documented as "spawn-free and timer-free" (`liquers-core/src/assets.rs`
module docs, "Manager implementations"; `PROJECT_OVERVIEW.md`; `DOC_03` "spawn-free contract"), and
`liquers-core/tests/manager_parametric.rs` proves it with `immediate_runs_without_tokio_runtime`
and `immediate_keyed_eval_without_tokio_runtime`, which run under `futures::executor::block_on`.

Both tests use `greet`, which neither logs nor reports progress. A command that does takes another
path: `Context::add_log_entry` (`context.rs`) sends `AssetServiceMessage::LogMessage`; the
service-message loop (`AssetRef::process_service_messages`, driven inline by `futures::join!`)
handles it with `AssetData::save_metadata_to_store`, which calls `MetadataSaver::save_immediately`.
On every non-wasm target that method spawns its coalescing worker with `tokio::spawn`, whatever the
manager's `EvalMode` — the choice is made by `#[cfg(not(target_arch = "wasm32"))]`, not by mode.
The same holds for `UpdatePrimaryProgress` / `UpdateSecondaryProgress`, and for a planner warning
(`Step::Info` / `Step::Warning` → `context.info` / `warning`).

`persist_with_status_tracking` already gets this right: it checks `EvalMode::Inline` and persists
synchronously. `MetadataSaver` does not.

**Inferred from code at commit 18732dd, not executed:** no existing test exercises it, and the bulk
design that found it may not add tests. The two existing tests pass (run 2026-10-20).

## Impact

An embedder that runs the immediate manager natively under another executor (`smol`,
`futures::executor`), or with no runtime, panics with "there is no reactor running" as soon as any
command logs, which most real commands do. Under a tokio runtime (every in-tree native use) nothing
is visible. wasm is unaffected (the spawn is compiled out there).

## Expected behaviour

Under `EvalMode::Inline`, `MetadataSaver::save_immediately` persists in the caller's task, as the
wasm branch does; the queued manager keeps its debounced background worker.

## Discovery

Bulk design 2026-10-20 (cluster C6), while measuring tokio use in `liquers-core` for
`CORE-TOKIO-REMOVAL` (`design/executor-neutral-core/`).
