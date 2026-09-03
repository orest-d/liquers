# Phase 1: High-Level Design — Sidecar-colliding keys refused by the path builders

## Feature Name

Sidecar-colliding keys refused by the path builders (`AsyncFileStore`, `FileStore`, `PathMap`)

## Purpose

`AsyncFileStore` keeps metadata in a sidecar (`foo.__metadata__`) and a lock beside it
(`foo.__lock__`), which makes keys using those names unaddressable. `is_supported` refuses them —
but `is_supported` is only a *routing hint*, and every fallible method builds its path without
asking, so `set("collide.__metadata__")` writes through and silently overwrites the metadata of
`collide`. This moves the refusal into the path builders, which is what `STORE_SEMANTICS.md` §8
already promises and what `AsyncOpenDALStore` already does — and widens the reserved-name rule from
the filename to **every segment**, so the legacy `parent/__metadata__/filename.json` layout stays
reachable.

## The rule, as settled at this gate

A store refuses a key when **any segment** is a name its metadata layout reserves. Each store
reserves what its own layout uses, in one predicate consulted by `is_supported` *and* by every path
builder:

| Store | Reserved in every segment |
|---|---|
| `AsyncFileStore` | `*.__metadata__`, `__metadata__`, `*.__lock__`, `__lock__` |
| `FileStore` | `*.__metadata__`, `__metadata__` |
| `AsyncOpenDALStore` (`PathMap`) | `*.__metadata__`, `__metadata__` — it has no lock files |

The bare-folder forms are reserved because earlier Liquers versions stored metadata in a
`__metadata__` folder and that layout may need to be supported again
(`STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`).

## Core Interactions

### Query System
None. The key grammar is unchanged; a key with a reserved segment still parses.

### Store System
The whole of the change, in `is_supported`, the path builders, and the listing filters — which
today drop `*.__metadata__` and `*.__lock__` by suffix and so would let a bare `__metadata__`
folder into `keys()` as a key the store then refuses. Conformance rule `sidecar03` stops failing on
`C2`; `prefix03` and `sibling05`, currently "not run" for that fixture, become runnable.

### Command System / Asset System / Value Types
None.

### Web/API
`liquers-axum`'s store handlers call the store directly (`store/handlers.rs:80`), which is how
`PUT /api/store/data/collide.__metadata__` reaches the filesystem today. After the fix they return
the store's refusal. No handler change.

### UI
None.

## Crate Placement

**liquers-core** — `src/store.rs` (both file stores), `tests/store_conformance_CONF.rs` (`C2` drops
its `AllowedFailure`, gains an `unsupported_shape` key).
**liquers-store** — `src/opendal_store.rs`: `PathMap::is_suffix_ambiguous` widens from the filename
to every segment, so the contract and both sidecar implementations agree.
Not `liquers-web`: the `localStorage` store namespaces data and metadata (`{ns}/{tag}/{key}`) and
has no collision.

Cross-crate, so `L` rather than the issue's recorded `M` — which is what this design folder is for.

## Documentation Intent

**Reference:** *Extend* `specs/reference/STORE_SEMANTICS.md` §8. The rule is written and correct as
far as it goes, but it is scoped to the filename and to `.__metadata__` alone. Restating it as
*reserved names, in any segment, declared by the store's metadata layout* — with the file stores'
`.__lock__` and the legacy folder form named as instances — plus a `## History` row and a
`reviewed:` bump.

**Guide:** *Extend* `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §"The key space". It already says
to refuse unrepresentable keys "from `is_supported` and from the path builders"; it does not say
*how*, and this issue is what happens when the two halves are written separately. Adding the
technique — one predicate, because `is_supported` returns `bool` and cannot carry an error — the
listing filter as the third caller, and the failure mode it prevents.

**Other documents to create:** None. `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` is filed and
carries the subsystem question; this design does not pre-empt it.

**Specific documents to update:**
- `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` → `status: closed`
- `specs/index.csv`, `specs/README.md` — this design, the closed issue, the new issue

Audience: whoever implements or reviews a store that keeps metadata beside its data. After this
they should learn the rule from `STORE_SEMANTICS.md` §8 and the technique from the guide, without
reading this folder.

## Resolved at this gate

- **Q1 — `.__lock__` is in scope.** A data file at `foo.__lock__` makes `acquire_lock(foo)` retry
  300 times and fail, so every later `set(foo)` blocks ~3 s and then errors, permanently. Worse
  than the metadata case, and `is_supported` already refuses it.
- **Q2 — `Error::key_not_supported` / `ErrorType::KeyNotSupported`**, matching
  `AsyncOpenDALStore::reject_ambiguous`. Not `KeyNotAbsolute`, which the traversal guard in the
  same functions raises and which would say the wrong thing.
- **Q3 — the obsolete synchronous `FileStore` gets the same fix.** Same copied bug, few lines;
  fixing one of two identical bugs invites the next reader to conclude the sync store is fine.
- **Q4 — interior segments are in scope** (user decision). `dir.__metadata__/child` passes
  `is_supported` today because `Key::filename()` is the last segment only. Reserving the bare
  `__metadata__` name too keeps the legacy folder layout available.
- **Q5 — a pluggable metadata layout is out of scope** (user decision), filed as
  `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` (P2, L). Its Impact section records that whatever
  implements it must revisit `is_supported` and the path builders, because the reserved-name set
  becomes a property of the configured layout rather than a constant.

## Open Questions

1. **Where does the shared predicate live?** The two file stores are in one module and can share
   one; `PathMap` is in another crate and keeps its own. A core-wide helper is the shape
   `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE` will want — is it worth building the seam now, or
   does that pre-empt a design that has not happened? Phase 2.
2. **How is the interior-segment case covered?** `GenericFixture` holds one `unsupported_shape`
   key, so a second shape needs either a fixture change, a new conformance rule (`sidecar04`), or a
   plain unit test in `store.rs`. Phase 2/3.
3. **Do the listing filters and `keys()` use the same predicate?** They must, or `keys()` returns
   keys the store refuses — a legacy `__metadata__` folder is exactly that case, since the current
   filter matches the suffix form only. Confirm the fix closes it. Phase 2.
4. **Does anything in the tree write a reserved key today?** Recipes, the axum bulk-upload handler
   and the web store browser all pass user-supplied names through. Phase 2 survey; a caller that
   breaks is the point of the change, but it should be a known list rather than a surprise.

## References

- `specs/issues/CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS.md` (P1, M, `core/store`)
- `specs/issues/STORE-METADATA-LAYOUT-HARDCODED-PER-STORE.md` (P2, L) — filed from this gate
- `specs/reference/STORE_SEMANTICS.md` §8, §1 (the directory form takes the same refusals)
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §"The key space"
- `liquers-store/src/opendal_store.rs` — `PathMap::is_suffix_ambiguous`, `reject_ambiguous`: the
  working instance of the pattern, including the `key_to_path_dir` case added in review of PR #58
- `liquers-core/src/store_conformance/rules/sidecar.rs` — `sidecar01`, `sidecar03`
- `specs/design/store-conformance-suite/` — the design that found this
