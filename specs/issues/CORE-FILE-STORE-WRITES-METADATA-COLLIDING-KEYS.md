---
id: CORE-FILE-STORE-WRITES-METADATA-COLLIDING-KEYS
kind: issue
title: AsyncFileStore refuses a sidecar-colliding key in is_supported but writes it in set
status: draft
priority: P1
complexity: M
area: [core/store]
design: store-conformance-suite
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
