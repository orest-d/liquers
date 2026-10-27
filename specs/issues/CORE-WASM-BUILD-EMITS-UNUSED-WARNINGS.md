---
id: CORE-WASM-BUILD-EMITS-UNUSED-WARNINGS
kind: issue
title: A wasm32 build of liquers-core emits five unused-import and dead-code warnings
status: closed
priority: P3
complexity: S
area: [core/assets, build]
design: core-wasm-warning-free
created: 2026-10-17
github:
---
## Problem

`cargo check -p liquers-core --target wasm32-unknown-unknown` (2026-10-17, main at the merge of
PR #139) emits five warnings that the native build does not:

- `liquers-core/src/assets.rs`: unused imports `AtomicUsize`, `VecDeque`, `scc`, `Notify` — used
  only by native-only code (`DefaultAssetManager`, the job queue);
- `field interval is never read` and `struct FileLockGuard is never constructed`.

## Problem Example

`cargo check -p liquers-core --target wasm32-unknown-unknown` prints five `liquers-core` warnings;
it should print none. Rechecked by code reading on 2026-10-20 (main 18732dd): the four imports in
`assets.rs:313-321`, `MetadataSaver::interval` (`assets.rs:438`, read only in the native
`save_task`) and `FileLockGuard` (`liquers-core/src/store.rs:1459`, which is in `store.rs`, not
`assets.rs`, and is constructed only in the native `impl AsyncFileStore`) are all still ungated.

## Impact

Noise in the wasm rows of `scripts/check-build-matrix.sh`, which hides a new warning that matters.

## Expected behaviour

The imports and items are gated with `#[cfg(not(target_arch = "wasm32"))]` like the code that uses
them, and the wasm32 build is warning-free.

## Discovery

Found while checking `design/runtime-volatility` on wasm32; not caused by it.

## Resolution

Fixed 2026-10-27 by `design/core-wasm-warning-free/`. Fixed by `design/core-wasm-warning-free/` (2026-10-27): the native-only imports (`VecDeque`, `AtomicUsize`, `scc`, `Notify`, and `PathBuf` in `store.rs`) and `FileLockGuard` are gated like their users, and `MetadataSaver::interval` allows dead code on wasm32. `cargo check -p liquers-core --target wasm32-unknown-unknown` prints no warning.
