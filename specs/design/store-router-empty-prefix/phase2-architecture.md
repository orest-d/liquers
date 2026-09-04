# Phase 2: Solution & Architecture - Empty File-Store Directories

## Overview

The absence rule is extended to directory listing: an absent, addressable directory returns
`Ok(vec![])`. `AsyncFileStore::listdir` and `FileStore::listdir` make that one behavioral change;
the existing `AsyncStore` and router defaults then make `keys()` work without router-specific
error handling. No types, trait methods, serialization, commands, or dependencies change.

## Known-Issue Preflight

| Issue | Status / priority | Relevance and action | Blocking? |
|---|---|---|---|
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | draft / P2 | Source issue. Its C3 reproduction is the acceptance case. | No |
| `CORE-FILE-STORE-LISTDIR-DROPS-METADATA-ONLY-KEYS` | draft / P2 | Same two listing methods, but independent handling of sidecars. Preserve the reserved-name filter. | No |
| `STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE` | draft / P2 | Same reference document but unrelated directory-metadata contract. Do not alter it. | No |

Searched `specs/issues/` and `specs/index.csv` for `core/store`, file-store listing, router, and
absence issues. No prerequisite invalidates this change; P2 remains appropriate because this is an
enumeration failure, not data loss, a panic, or an incorrect write.

## Exact Behavior

In both existing `listdir` implementations in `liquers-core/src/store.rs`, retain the current
`key_to_path(key)?` guard. When `try_exists(path)` successfully returns `false`, return
`Ok(vec![])` instead of `Err(Error::key_not_found(key))`. Leave the `try_exists`, `metadata`, and
`read_dir` error mappings unchanged: permission, I/O, and races remain `KeyReadError` through the
existing typed constructor. A present non-directory remains the current `Ok(vec![])` behavior.

The behavior applies to any absent addressable directory, not merely a member root. The methods
cannot distinguish a configured prefix from another directory, and the contract should not create
that artificial distinction. A relative or reserved key still fails before this branch through the
existing path builder; it is not normalized as absence.

## Trait, Ownership, and Async Decisions

No new struct, enum, generic, trait implementation, public signature, allocation, or serialization
is introduced. The async method remains async because it performs filesystem I/O; the synchronous
method mirrors its result contract. `AsyncStoreRouter` remains unchanged and object-safe.

## Data Structures

None. The change alters only two existing branch results and persists no new state.

## Trait Implementations

None. `AsyncFileStore`, `FileStore`, and `AsyncStoreRouter` retain their existing implementations
and use the existing `AsyncStore::keys` default.

## Sync vs Async

`AsyncFileStore::listdir` stays async because it calls Tokio filesystem APIs. `FileStore::listdir`
remains synchronous and receives the same semantic branch; no async wrapper is introduced.

## Function Signatures

No signature changes. Both methods remain `listdir(&self, key: &Key) -> Result<Vec<String>, Error>`
under their existing async or synchronous trait implementation.

## Integration Points

Modify only `liquers-core/src/store.rs`; amend the C3 harness in
`liquers-core/tests/store_conformance_CONF.rs`. No downstream crate or feature-gate changes.

## Relevant Commands

None. Stores are invoked through the Rust `AsyncStore` API; no Liquers command namespace or query
syntax is involved.

## Error Handling

The new success branch is entered only when `try_exists` returns `Ok(false)`. Existing
`Error::key_read_error` mappings remain responsible for failed filesystem operations; no direct
`Error::new` construction or error-type conversion is added.

## Alternatives Rejected

- Catch `KeyNotFound` in `AsyncStoreRouter::keys`: fixes only one caller, conceals a direct
  `AsyncFileStore::listdir` inconsistency, and makes router semantics differ from store semantics.
- Pre-create member directories: deployment setup should not be required to make enumeration work.
- Catch every listing error: would turn permission and I/O failures into false empty directories.

## Documentation Architecture

| Path | Kind / audience | Exact change |
|---|---|---|
| `specs/reference/STORE_SEMANTICS.md` | reference / internal | Extend §4's absence table and explanation with `listdir -> Ok([])` for an absent, addressable directory; retain `Err` for backend failure. |
| `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` | guide / internal | Add the `listdir` case to the absence/error-mapping checklist and the focused conformance command. |

`affects_docs` is exactly these two paths. Phase 5 will add History rows, set `reviewed: 2026-09-04` after checking the final code, confirm their claims against the implementation, close the issue, and update the capability map only if a new capability entry is warranted (it is not expected for this bug fix).

## Rust Best-Practices Review

No blocking concern: the plan keeps I/O async, creates no error with `Error::new`, preserves the
existing `Result<_, Error>` signatures, and does not change trait-object bounds or crate flow.
