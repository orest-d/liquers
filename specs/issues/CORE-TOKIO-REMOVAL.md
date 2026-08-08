---
id: CORE-TOKIO-REMOVAL
kind: issue
title: Core still depends on tokio primitives on wasm
status: draft
priority: P3
complexity: XL
area: [core/assets]
design: 
created: 2026-08-08
github:
---
## Problem

`async-wasm-refactor` made `liquers-core` run in the browser, but wasm still uses `tokio::sync`
for the channels and locks in `AssetData` and `DependencyManager`. The core is therefore tied to
tokio even where no tokio runtime exists.

## Impact

The core cannot run under another executor — embassy, smol, `futures-executor` — which closes off
the embedded angle. Nothing is broken today; this is a constraint on where Liquers can go.

## Expected behaviour

Replace the tokio primitives with framework-neutral ones (`async-lock`, `async-channel`,
`event-listener`, `async-once-cell`) so the core is executor-agnostic. See
`specs/async-wasm-refactor/phase2-architecture.md` → "Tokio Dependency Reduction".

Wants a design: it touches every await point in the asset lifecycle.

## Discovery

Migration triage, 2026-08-08. Source: the *async-wasm-refactor follow-ups* section of `specs/archive/2026-08-08-issues.md`, deliberately out of scope at the time. Verified against HEAD — see
`specs/DOCS_MIGRATION_PLAN.md` §4.0c.
