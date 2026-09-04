---
id: CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS
kind: issue
title: AsyncFileStore listings drop a metadata-only key instead of reporting it
status: draft
priority: P2
complexity: S
area: [core/store, store/backends, docs]
design:
created: 2026-09-03
github:
---
## Problem

`STORE_SEMANTICS.md` §8 says a sidecar implies its data key:

> A sidecar found in the backend implies its data key: a listing reports `sub/orphan.__metadata__`
> as `sub/orphan`.

`AsyncOpenDALStore` does this — `PathMap::decode` strips the suffix, so the sidecar and the data
object both decode to `sub/orphan` and `listdir` reports the name once
(`liquers-store/src/opendal_store.rs:459-469`).

`AsyncFileStore` does not. Its `listdir` **drops** the sidecar name and reports nothing in its place
(`liquers-core/src/store.rs:1209`):

```rust
if !(name.ends_with(Self::METADATA) || name.ends_with(Self::LOCK)) {
    names.push(name);
}
```

`FileStore` has the same line at `store.rs:1444`.

So a key that has metadata and no data is invisible to the file stores' listings. That state is
reachable through the ordinary API: `set_metadata(k, …)` writes only the sidecar, and `liquers-axum`
exposes it as `PUT /api/store/metadata/{key}` (`liquers-axum/src/store/handlers.rs:211`).

## Impact

The store answers two questions inconsistently about the same key:

| Call | Metadata-only key `sub/orphan` |
|---|---|
| `contains("sub/orphan")` | `true` — it checks the data path, then the metadata path (`store.rs:1158-1170`) |
| `get_metadata("sub/orphan")` | `Ok` — the sidecar is there and parses |
| `listdir("sub")` | omits it |
| `keys()` | omits it, since `keys()` is built on `listdir` |

A caller enumerating a store therefore cannot see assets whose data has not been written yet — a
recipe whose metadata was recorded before evaluation, or an upload that set metadata first and then
failed. Worse, the two in-tree sidecar stores disagree: the same sequence of calls against
`AsyncOpenDALStore` lists the key and against `AsyncFileStore` does not, so behaviour changes when a
deployment moves from a local folder to object storage.

No workaround beyond calling `contains` on a key you already guessed.

## Expected behaviour

`AsyncFileStore::listdir` and `FileStore::listdir` report the implied data key for a sidecar they
find — strip the suffix and emit that name — rather than dropping it, de-duplicating against the
data file when both exist. This is what `PathMap::decode` already does, and §8 already requires.

The lock suffix keeps its current treatment: a lock file is transient bookkeeping and implies no
key, so it is still dropped outright.

**Add a conformance rule.** §8 states this and `sidecar01`-`sidecar03` do not check it, which is why
two implementations could diverge unnoticed. A rule in the `sidecar` family, at `CreateOnly` (write
metadata for a fresh key, then look for it in `listdir`/`keys`), would have caught it.

`S`: two `listdir` bodies and one rule.

## Discovery

Found on 2026-09-03 during the corner-case analysis of `SIDECAR-COLLIDING-KEYS`, while tracing what
the file stores' listing filters do with each reserved name. That design changes the *predicate*
those two lines use but deliberately not their drop-versus-report behaviour, so it leaves this
untouched and is not blocked by it.
