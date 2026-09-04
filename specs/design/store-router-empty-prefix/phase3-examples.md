# Phase 3: Examples & Tests - Empty File-Store Directories

## Overview Table

The examples progress from an empty file-store directory, to a mixed router containing that empty
member, to the boundaries that must not be softened. They are executable tests, not user-facing
examples; no guide snippet is needed beyond the implementation-guide checklist.

| Evidence | Location | Demonstrates |
|---|---|---|
| `filestore01` async unit test | `liquers-core/src/store.rs` tests | An absent addressable directory lists as `[]`, `is_dir` remains false, and `keys()` contains only the store prefix. |
| `filestore02` sync unit test | `liquers-core/src/store.rs` tests | `FileStore` has the same absent-directory listing contract. |
| C3 router regression | `liquers-core/tests/store_conformance_CONF.rs` | A router with `mem` and an uncreated `files` prefix completes `keys01`/`keys02`; no setup directory masks the result. |
| Boundary coverage | existing key-absolute and file-store tests | Relative/reserved keys still fail through `key_to_path`; a present file still lists as `[]`; real filesystem errors retain their existing mapping. |

## Example 1 - Direct Async File Store

The asynchronous store at prefix `files` has no `files/` path on disk. Listing it succeeds with
no names, while `is_dir` remains false; this distinguishes an empty namespace from a directory
object and from `KeyNotFound` reads.

## Example 2 - Mixed Router

The C3 router has `mem` and `files` members, but only the temporary backend root exists. Its normal
conformance run reaches enumeration and succeeds, proving that the empty `files` member contributes
only its namespace and does not abort keys from the router.

## Corner Cases

An existing ordinary file remains a non-directory with an empty listing. Relative or reserved keys
still fail before filesystem existence is checked. The plan intentionally does not assert a
platform-specific permission failure, but preserves the existing typed error mapping for it.

## Test Plan

`filestore01` creates only a temporary backend root, constructs `AsyncFileStore(root, "files")`,
and calls `listdir("files")` without creating `root/files`. Assert `Ok([])`,
`is_dir("files") == false`, and `keys() == ["files"]` (order as returned by the trait default).

`filestore02` performs the same assertion for `FileStore`; it is required because the shared
contract covers the synchronous implementation even though the reported router failure is async.

For C3, delete `create_dir_all(root.join("files"))`. Keep the fixture and its full `run_all`
report unchanged: its enumeration rules are the end-to-end proof that the router reports `mem`,
`files`, and root rather than propagating `KeyNotFound` from `files`.

Do not add a portable permission-denied test: filesystem permissions vary by platform and CI user.
Instead retain the existing `map_err(Error::key_read_error)` paths unchanged and review that the
new `Ok([])` branch is reached only after a successful `try_exists == false` result.

## Validation

```text
cargo test -p liquers-core --lib filestore01
cargo test -p liquers-core --lib filestore02
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF c3_async_store_router
cargo test -p liquers-core --features store-conformance --test store_conformance_CONF
```

The feature flag is mandatory: without it the C3 test target compiles but runs zero tests.

## Documentation and Learning Log

Phase 5 will record the final contract in `STORE_SEMANTICS`, the implementation advice in
`STORE_IMPLEMENTATION_GUIDE`, the source-issue resolution, and whether the direct tests exposed
any backend-specific behavior that changes this plan.
