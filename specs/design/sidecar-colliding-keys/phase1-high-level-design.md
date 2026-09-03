# Phase 1: High-Level Design — Sidecar-colliding keys refused by the path builders

## Feature Name

Sidecar-colliding keys refused by the path builders (`AsyncFileStore`, `FileStore`)

## Purpose

`AsyncFileStore` keeps metadata in a sidecar (`foo.__metadata__`) and a lock beside it
(`foo.__lock__`), which makes keys ending in those suffixes unaddressable. `is_supported` refuses
them — but `is_supported` is only a *routing hint*, and every fallible method builds its path
without asking, so `set("collide.__metadata__")` writes through and silently overwrites the
metadata of `collide`. This moves the refusal into `key_to_path` / `key_to_path_metadata` /
`key_to_lock_path`, which is what `STORE_SEMANTICS.md` §8 already promises and what
`AsyncOpenDALStore` already does.

## Core Interactions

### Query System
None. The key grammar is unchanged; a key ending in `.__metadata__` still parses.

### Store System
The whole of the change. `AsyncFileStore` and the obsolete synchronous `FileStore` gain one
predicate over the reserved suffixes, consulted by `is_supported` (as today) *and* by every path
builder (new). Conformance rule `sidecar03` stops failing on `C2`; `prefix03` and `sibling05`,
currently "not run" for that fixture, become runnable.

### Command System / Asset System / Value Types
None.

### Web/API
`liquers-axum`'s store handlers call the store directly, which is how `PUT
/api/store/data/collide.__metadata__` reaches the filesystem today. After the fix they return the
store's refusal instead. No handler change.

### UI
None.

## Crate Placement

**liquers-core** — `src/store.rs` (both file stores) and `tests/store_conformance_CONF.rs` (`C2`
drops its `AllowedFailure`, gains an `unsupported_shape` key). Nothing above core changes:
`AsyncOpenDALStore` in `liquers-store` is already correct and is the model being followed; the
`localStorage` store in `liquers-web` namespaces data and metadata separately and has no collision.

## Documentation Intent

**Reference:** *Extend* `specs/reference/STORE_SEMANTICS.md` §8. The rule is already written and
already correct — what it lacks is that `.__metadata__` is not the only reserved suffix, since
`AsyncFileStore` also reserves `.__lock__` with the same collision and a worse consequence (a data
file at `foo.__lock__` makes every later `set(foo)` block and then time out). Generalizing §8 to
*reserved sidecar suffixes*, with both instances named, plus a `## History` row and a `reviewed:`
bump.

**Guide:** *Extend* `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §"The key space". It already says
to refuse unrepresentable keys "from `is_supported` and from the path builders"; it does not say
*how*, and this issue is exactly what happens when the two halves are written separately. Adding
the technique — one predicate, because `is_supported` returns `bool` and cannot carry an error —
and the failure mode it prevents.

**Other documents to create:** None. The fix is small and its lessons belong in the two documents
above, not in a new one.

**Specific documents to update:**
- `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` → `status: closed`
- `specs/index.csv` — the closed issue, this design
- `specs/README.md` — the design entry (§8.5)
- Any new issue filed for scope deliberately left out (see Q4)

Audience: whoever implements or reviews a sidecar-backed store. After this, they should learn the
rule from `STORE_SEMANTICS.md` §8 and the technique from the guide, without reading this folder.

## Open Questions

1. **Is `.__lock__` in scope, or only `.__metadata__`?** The issue names metadata only.
   Proposal: both — `is_supported` already refuses both, so the path builders mirroring it is the
   stated fix, and the lock collision is the more damaging of the two.
2. **Which error?** Proposal: `Error::key_not_supported(key, &self.store_name())` /
   `ErrorType::KeyNotSupported`, matching `AsyncOpenDALStore::reject_ambiguous`. (`KeyNotAbsolute`,
   which the traversal guard in the same functions raises, would say the wrong thing.)
3. **Does the obsolete synchronous `FileStore` get the same fix?** It is unreachable
   (`CORE-SYNC-STORE-TRAIT-OBSOLETE`, P2/M) and has no conformance fixture, but it carries the same
   bug in copied code. Proposal: yes — a few lines, and leaving one of two identical bugs fixed is
   how the next reader concludes the sync store is fine.
4. **Interior segments.** `dir.__metadata__/child` passes `is_supported` (`Key::filename()` is the
   *last* segment only) and its parent directory collides with `dir`'s metadata *file*. It cannot
   corrupt anything — the filesystem refuses to be both — but it turns `set_metadata("dir")` into
   an unexplained `KeyWriteError`. Proposal: out of scope, filed as its own issue; §8 and
   `PathMap::is_suffix_ambiguous` are both filename-scoped, so widening the rule is a contract
   change, not a bug fix.
5. **Shared or duplicated predicate?** The suffix list would live in three places
   (`AsyncFileStore`, `FileStore`, `PathMap`). Proposal: leave `PathMap` alone and share only
   between the two file stores, if that costs nothing; a core-wide sidecar helper is a refactor
   this issue does not need. Settle in Phase 2.

## References

- `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` (P1, M, `core/store`)
- `specs/reference/STORE_SEMANTICS.md` §8, §1 (the directory form takes the same refusals)
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §"The key space"
- `liquers-store/src/opendal_store.rs` — `PathMap::is_suffix_ambiguous`, `reject_ambiguous`: the
  working instance of the pattern, including the `key_to_path_dir` case added in review of PR #58
- `liquers-core/src/store_conformance/rules/sidecar.rs` — `sidecar01`, `sidecar03`
- `specs/design/store-conformance-suite/` — the design that found this
