---
id: CORE-TOKIO-REMOVAL
kind: issue
title: Core still depends on tokio primitives on wasm
status: accepted
priority: P3
complexity: XL
area: [core/assets]
design: executor-neutral-core
created: 2026-08-08
github:
---
## Problem

> **Re-scoped 2026-10-20 (bulk design, `design/executor-neutral-core/`).** Partly inaccurate as
> written. `tokio::sync` primitives are runtime-agnostic, so they do not tie the core to tokio; on
> wasm the core already needs only `tokio` with feature `sync` and no runtime. What does tie it:
> `tokio::spawn` (9 library sites) and `tokio::time` (3), all chosen by `target_arch` rather than
> by the manager's mode. So the queued `DefaultAssetManager` needs a tokio runtime, and the native
> immediate path still spawns in two places (`MetadataSaver::save_immediately`, filed as
> `METADATA-SAVER-SPAWNS-ON-THE-INLINE-PATH`; `AssetRef::new_temporary`) and uses a tokio timer
> in `AssetRef::cancel`. The design's blocking question is which goal this issue means:
> executor-agnostic (spawn/timer seam, `tokio::sync` kept) or tokio-free (replace `tokio::sync`).
> The embassy / embedded angle is out of reach either way (`std` throughout). Original text kept
> below as history.


`async-wasm-refactor` made `liquers-core` run in the browser, but wasm still uses `tokio::sync`
for the channels and locks in `AssetData` and `DependencyManager`. The core is therefore tied to
tokio even where no tokio runtime exists.

## Impact

The core cannot run under another executor — embassy, smol, `futures-executor` — which closes off
the embedded angle. Nothing is broken today; this is a constraint on where Liquers can go.

## Expected behaviour

Replace the tokio primitives with framework-neutral ones (`async-lock`, `async-channel`,
`event-listener`, `async-once-cell`) so the core is executor-agnostic. See
`specs/design/async-wasm-refactor/phase2-architecture.md` → "Tokio Dependency Reduction".

Wants a design: it touches every await point in the asset lifecycle.

## Discovery

Migration triage, 2026-08-08. Source: the *async-wasm-refactor follow-ups* section of `specs/archive/2026-08-08-issues.md`, deliberately out of scope at the time. Verified against HEAD — see
`specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
