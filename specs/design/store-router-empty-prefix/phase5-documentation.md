# Phase 5: Documentation - Empty File-Store Directories

## Completion Preconditions

Implementation is complete. The two direct file-store tests and the complete feature-gated core
conformance target pass; the C3 reproduction no longer creates its missing `files` directory.

## Implementation Summary

`AsyncFileStore::listdir` and `FileStore::listdir` now return an empty list when their checked,
addressable filesystem path is absent. They still propagate failures from existence checks,
metadata reads, and directory reads. `AsyncStoreRouter` required no code change: its existing
recursive listing now sees an empty member namespace rather than `KeyNotFound`.

Two regression tests, `filestore01` and `filestore02`, cover the async and synchronous stores.
C3 now exercises the original empty-prefix shape directly. No requested scope was omitted and no
scope was added beyond the synchronous twin required by the shared store contract.

## Documentation Delivered

- `specs/reference/STORE_SEMANTICS.md`: §4 defines `listdir` absence as `Ok([])` and distinguishes
  it from a backend failure.
- `specs/guides/STORE_IMPLEMENTATION_GUIDE.md`: the error-mapping checklist explains that a
  listing must not turn every filesystem error into an empty namespace.

These are the authoritative `affects_docs` documents. Both were reviewed against the final code on
2026-09-04 and received matching History rows. No capability-map entry changed because this is a
bug fix to existing store behavior.

## Issues Filed

None. `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` is closed with its test evidence.

## Important Learning

The async router failure was the visible symptom, but the correct boundary was file-store listing.
Fixing it there keeps direct and routed behavior consistent, and mirroring it in `FileStore` avoids
leaving the stated store contract split by sync versus async implementation.

## Conformance and Remaining Work

Conforms to the approved design. The pre-existing `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS`
remains independent and unmodified. Workspace-wide `cargo fmt --check` remains red because of
unrelated formatting drift outside this change; all touched behavior is covered by focused tests.

## Validation

```text
cargo test -p liquers-core --lib filestore01
cargo test -p liquers-core --lib filestore02
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF c3_async_store_router
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF
```

All commands passed on 2026-09-04.
