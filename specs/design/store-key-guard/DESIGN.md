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

**Naming, checked against RFC 430 / Rust API Guidelines C-CONV (asked 2026-08-17):**

| Prefix | Guideline | Signature it fits |
|---|---|---|
| `as_` | free, **borrowed → borrowed** | `fn as_absolute(&self) -> Result<&Key, Error>` |
| `to_` | expensive, borrowed → owned | (already used: `to_absolute(&self, cwd) -> Key`) |
| `into_` | **consuming**, owned → owned | `fn try_into_absolute(self) -> Result<Key, Error>` |

`try_` is the fallible prefix, after `TryFrom`/`TryInto`. So for a method that *consumes* self the
convention-correct name is `try_into_absolute`, not `as_absolute` — `as_` promises a cheap borrow,
which a consuming signature is not.

Both are proposed, because they are not interchangeable at the call site. `AsyncStore`'s methods
take `&Key`, so a consuming check would force `key.clone()` on every store operation purely to
validate. `as_absolute` avoids the clone and keeps the benefit that motivated the consuming form:
`let key = key.as_absolute()?;` shadows the parameter, so the unchecked key cannot be used by
accident afterwards. `try_into_absolute` is one line over it, for call sites that already own a key.

House style deviates from the guideline in one respect worth noting: `ValueInterface`'s
`try_into_*` methods take `&self` (`value.rs:139` onwards). The proposal follows the guideline
rather than that precedent, since here both forms exist and the distinction is the point.

**Risk this creates:** `to_absolute(cwd)` *resolves* and `as_absolute()` *asserts* — one word
apart, opposite behaviour. Cross-referencing rustdoc on both is a requirement, not a nicety.

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
