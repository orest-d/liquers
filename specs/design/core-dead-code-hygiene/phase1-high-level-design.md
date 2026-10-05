# Phase 1: High-Level Design - Account for `entities.rs` and `cache.rs`

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** Re-audited at HEAD on 2026-10-05. `entities.rs` is live; `cache.rs` is obsolete
  and has one compiled consumer, a legacy `liquers-py` API whose removal is already the scope of
  `CORE-SYNC-STORE-TRAIT-OBSOLETE`. The issue's own acceptance rule ("used, deleted, or carries a
  comment saying what it is for") is met by documenting `cache.rs` and assigning its deletion to
  that issue. Rewritten after the post-Phase-4 review, which found the first version to be
  template text.
- **Open questions:** None

## Problem and Evidence

`REPO-DEAD-CODE-HYGIENE` names `liquers-core/src/entities.rs` and `liquers-core/src/cache.rs` as
removal candidates (work package WP-13), not re-verified since.

| Module | Callers at HEAD (2026-10-05) | Finding |
|---|---|---|
| `entities.rs` (+ `entities/codegen.rs`) | `escape.rs` (`entities::curated_name` ≈262, `entities::lookup` ≈423, `entities::compiled_count` ≈481/≈490); `bin/generate_entities.rs` (`entities::codegen`); documented in `DOC_02_QUERY_LANGUAGE_REFERENCE.md` ≈98 | **Live**, with a full module doc. Nothing to do |
| `cache.rs` (`BinCache`, `Cache<V>`, `NoBinCache`, `MemoryBinCache`, `NoCache`, `SerializingCache`) | no caller inside `liquers-core`; one compiled consumer: `liquers-py/src/context.rs` (`Environment.cache: Arc<Mutex<Box<dyn Cache<Value>>>>`, `with_cache`, ≈4/≈32/≈46/≈74); plus `liquers-py/src/cache.rs`, an undeclared orphan that never compiles (`PY-MODULES-NOT-DECLARED-IN-LIB`) | **Obsolete but reachable.** Assets provide caching; `LANGUAGE-INTEGRATION_GUIDE.md` ≈244 already calls the module legacy and scheduled for removal. Nothing in the module says so, it begins with blanket `#![allow(unused_imports)]` / `#![allow(dead_code)]` and an unused `use chrono::format;`, and `PROJECT_OVERVIEW.md` ≈103 still describes it as "Query result caching" |

Deleting `cache.rs` now would change `liquers-py`'s public API (`Environment::with_cache`) — the
same kind of change `CORE-SYNC-STORE-TRAIT-OBSOLETE` plans for the sync `Store`, in the same
`liquers-py` `Environment`. One `liquers-py` API change is better than two.

## Expected Behaviour and Acceptance Criteria

1. `cache.rs` begins with a module doc comment stating: legacy synchronous cache; obsolete because
   assets provide caching; its only consumer is `liquers-py`'s legacy `Environment`; removal is
   tracked in `CORE-SYNC-STORE-TRAIT-OBSOLETE`; new code must not use it.
2. The unused `use chrono::format;` is removed; no other code change.
3. `PROJECT_OVERVIEW.md`'s module table describes `cache.rs` truthfully.
4. `CORE-SYNC-STORE-TRAIT-OBSOLETE` records that its removal also covers `liquers_core::cache` and
   `liquers-py`'s `Environment.cache` / `with_cache`.
5. `REPO-DEAD-CODE-HYGIENE` is closed with the evidence table above.

## Scope and Non-Goals

Non-goals: deleting `cache.rs` (owned by `CORE-SYNC-STORE-TRAIT-OBSOLETE`); deleting the orphan
`liquers-py/src/cache.rs` (owned by `PY-MODULES-NOT-DECLARED-IN-LIB`); changing `entities.rs`.

## Compatibility

No API or behaviour change.

## Documentation Assessment

Update `reference/PROJECT_OVERVIEW.md` (one table row; History row + `reviewed:`). No new document.

## Design Dependencies

| Relationship | Target | Effect |
|---|---|---|
| owns | `REPO-DEAD-CODE-HYGIENE` | closed by this design |
| covered-by (deletion of `cache.rs`) | `CORE-SYNC-STORE-TRAIT-OBSOLETE` | the module is removed with the sync `Store` and the `liquers-py` `Environment` fields |
| overlaps | `PY-MODULES-NOT-DECLARED-IN-LIB` | owns the orphan `liquers-py/src/cache.rs` |
