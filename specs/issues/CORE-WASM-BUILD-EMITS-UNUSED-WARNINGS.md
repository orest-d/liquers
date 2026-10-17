---
id: CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS
kind: issue
title: A wasm32 build of liquers-core emits five unused-import and dead-code warnings
status: draft
priority: P3
complexity: S
area: [core/assets, build]
design:
created: 2026-10-17
github:
---
## Problem

`cargo check -p liquers-core --target wasm32-unknown-unknown` (2026-10-17, main at the merge of
PR #139) emits five warnings that the native build does not:

- `liquers-core/src/assets.rs`: unused imports `AtomicUsize`, `VecDeque`, `scc`, `Notify` — used
  only by native-only code (`DefaultAssetManager`, the job queue);
- `field interval is never read` and `struct FileLockGuard is never constructed`.

## Impact

Noise in the wasm rows of `scripts/check-build-matrix.sh`, which hides a new warning that matters.

## Expected behaviour

The imports and items are gated with `#[cfg(not(target_arch = "wasm32"))]` like the code that uses
them, and the wasm32 build is warning-free.

## Discovery

Found while checking `design/runtime-volatility` on wasm32; not caused by it.
