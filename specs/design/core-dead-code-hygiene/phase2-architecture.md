# Phase 2: Solution and Architecture - Account for `entities.rs` and `cache.rs`

## Chosen Solution

1. **`liquers-core/src/cache.rs`** — insert a module doc comment before the two `#![allow]`
   attributes:

   ```rust
   //! Legacy synchronous query cache (`BinCache`, `Cache`). **Obsolete: do not use in new code.**
   //!
   //! Assets provide caching (`crate::assets`); nothing in `liquers-core` uses this module. Its one
   //! remaining consumer is `liquers-py`'s legacy `Environment` (`cache` field, `with_cache`), and it
   //! is removed together with the synchronous `Store` trait and that environment's fields
   //! (`CORE-SYNC-STORE-TRAIT-OBSOLETE`).
   ```

   Delete `use chrono::format;`. Leave the other imports and both `#![allow]` attributes: the module
   is going away, and tightening lints on it is churn.

2. **`specs/reference/PROJECT_OVERVIEW.md`** ≈103: the row becomes
   `| cache.rs | ~350 | Legacy synchronous cache; obsolete (assets cache results), removal tracked in CORE-SYNC-STORE-TRAIT-OBSOLETE |`.

3. **`specs/issues/CORE-SYNC-STORE-TRAIT-OBSOLETE.md`** — records that its removal covers
   `liquers_core::cache` and `liquers-py`'s `Environment.cache` / `with_cache` (§Related, added with
   this design on 2026-10-05).

## Rejected Alternatives

- **Delete `cache.rs` now** — forces a `liquers-py` public API change separate from the one
  `CORE-SYNC-STORE-TRAIT-OBSOLETE` already plans for the same struct.
- **`#[deprecated]` on the items** — every use in `liquers-py` would warn until the removal, adding
  noise without new information; the doc comment and the tracking issue carry the decision.
- **Delete `entities.rs`** — it is live.

## Files and Symbols

| File | Change |
|---|---|
| `liquers-core/src/cache.rs` | module doc; remove one import |
| `specs/reference/PROJECT_OVERVIEW.md` | one row; History + `reviewed:` |
| `specs/issues/CORE-SYNC-STORE-TRAIT-OBSOLETE.md` | scope note |
| `specs/issues/REPO-DEAD-CODE-HYGIENE.md` | `status: closed`, resolution |

## Risk Table

| Aspect | Assessment |
|---|---|
| Compatibility | none |
| Build | removing an unused import cannot break a build; `cargo check` proves it |
| Recovery | revert |
| Certainty | high |
