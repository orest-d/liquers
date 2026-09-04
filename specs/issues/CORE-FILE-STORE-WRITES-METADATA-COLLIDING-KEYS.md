---
id: CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS
kind: issue
title: AsyncFileStore refuses a sidecar-colliding key in is_supported but writes it in set
status: closed
priority: P1
complexity: L
area: [core/store]
design: sidecar-colliding-keys
created: 2026-09-02
github:
---
## Problem

`AsyncFileStore` keeps metadata in a sidecar: the metadata for `foo` lives at `foo.__metadata__`.
That makes the key `collide.__metadata__` unrepresentable — its *data* path is byte-identical to the
*metadata* path of the key `collide`.

`is_supported` refuses such a key. **`set` does not.** Conformance rule `sidecar03`:

```
FAILED sidecar03 [§8] set(collide.__metadata__) succeeded though is_supported refuses it.
```

`is_supported` is a **routing hint** — `AsyncStoreRouter` consults it, but a caller reaching the
store directly does not. `liquers-axum`'s store handlers call the store, so
`PUT /api/store/data/collide.__metadata__` writes through.

## Impact

**Silent metadata corruption.** Writing `collide.__metadata__` overwrites the metadata of `collide`
with arbitrary bytes. A later `get_metadata("collide")` either fails to parse or returns whatever
was written; the data of `collide` is untouched, so the corruption shows up as a file that exists
and cannot be described.

`STORE_SEMANTICS.md` §8 is explicit that such keys are refused "by `is_supported` **and by the path
builders** alike" — the path builders are the half that is missing. `AsyncOpenDALStore` does refuse
them, in `PathMap::is_suffix_ambiguous`, so this is `AsyncFileStore` alone.

## Expected behaviour

`key_to_path` and `key_to_path_metadata` refuse a key whose filename ends with the metadata suffix,
so every fallible method inherits the refusal — which is what "the path builders" means and how
`AsyncOpenDALStore` achieves it.

Sized `M` rather than `S` because it changes which keys a store accepts: anything currently writing
such a key starts failing, and that is the point, but it deserves its own change rather than riding
along in a test-suite PR. Recorded as an allowed failure on `C2` meanwhile, so `H5` reports it the
moment it is fixed.

## Discovery

Found on 2026-09-02 by conformance rule `sidecar03`, which exists because a Codex review of PR #59
pointed out that `sidecar01` checked only `is_supported` and "a sidecar-backed implementation that
reports false here yet accepts the key in `set` would still pass". It does, and it did.

## Resolution

Closed 2026-09-03 by `specs/design/sidecar-colliding-keys/`.

`ReservedNames` in `liquers-core::store` now owns the rule, and `is_supported`, the path builders
and the listing filters all consult it — in `AsyncFileStore`, `FileStore` and
`AsyncOpenDALStore`. `acquire_lock` builds the lock path first, so `set`, `set_metadata`, `remove`
and `removedir` refuse before any directory is created or byte written; there is no half-done
state. `C2`'s allowed failure for `sidecar03` is gone.

Two things went beyond what this issue described, both settled at the design's gates:

- **The rule covers every segment, not just the filename**, and reserves the bare `__metadata__`
  folder name as well as the `.__metadata__` suffix. The predecessor Python implementation
  (`orest-d/liquer`, `liquer/store.py`) refuses the name as a filename *and* in any interior
  position and filters it from listings; the Rust port had narrowed all three. This restores them.
- **The listing filters were not optional.** `listdir_keys_deep` calls `is_dir` on every child, so
  guarding the path builders alone would have turned silent corruption into a store whose `keys()`
  fails outright.

`complexity` raised `M` → `L`: the change reaches `liquers-store` and removes a `pub fn`.
`design` re-pointed from `store-conformance-suite`, which found this, to the one that fixed it.
