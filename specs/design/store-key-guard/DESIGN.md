---
id: STORE-KEY-GUARD
kind: design
title: Absolute store keys — a store refuses a relative key with a dedicated error
workflow: liquers-project
status: draft
phase: high-level
area: [core/store, store/backends, core/error, web, axum]
gh_pr: []
issues: [STORE-FILESTORE-PATH-TRAVERSAL]
affects_docs: []
created: 2026-08-17
superseded_by:
---
# store-key-guard Design Tracking

**Created:** 2026-08-17

## Phase Status

- [x] Phase 1: High-Level Design — awaiting approval
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Notes

**Phase 1 findings (verified at HEAD, not taken from the issue text):**

- The issue's line references have drifted. `AsyncFileStore::is_supported` is
  `liquers-core/src/store.rs:1159` and already checks prefix plus the metadata/lock suffixes — it is
  not the unconditional `true` at `:809`, which is `AsyncMemoryStore`. The `key_to_path` shape it
  describes is unchanged (`:835`, `:1185`).
- `is_supported` is consulted **only** by `StoreRouter::find_store` and `AsyncStoreRouter::find_store`
  (`store.rs:1579`, `:1588`, `:1793`). No store method calls it. So fixing `is_supported` alone
  leaves a directly-held `AsyncFileStore` exploitable — this rules out issue option 2 as sufficient
  and shapes Phase 2's open question 1.
- Confirmed reachable at HEAD with `liquers-validate`: `-R/../../etc/passwd` and
  `-R/a/../../etc/passwd` both parse and plan clean as `GetAsset`.
- `CwdCursor::is_relative` (`query.rs:2187`) inspects only the **first** segment, and
  `resolve_key` returns the key untouched when it is not relative. So `a/../../etc/passwd` is never
  normalized on any path — the recent CWD work (`b4de249`) does not cover it.
- Preliminary answer to open question 4: every in-tree dot-segment key found is pre-store — CWD
  resolution in `context.rs`/`interpreter.rs`, resolved by `resolve_key_from_cwd` before any store
  call. To be confirmed properly in Phase 2.
- `liquers-web/src/store/key_guard.rs` already implements the intended rule and its module docs
  name this issue as the reason it is a temporary local copy.

**Framing set by the user, 2026-08-17:** the rule is not a list of refused segments but a
precondition — relative keys are a plan-level feature, and a store requires an absolute key. A
dedicated error names the violation. Phase 1 rewritten around this; two naming collisions it runs
into are open questions 1 and 2 (`CwdCursor::is_relative` tests only the first segment;
`Query::absolute` already means "had a leading `/`").

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
