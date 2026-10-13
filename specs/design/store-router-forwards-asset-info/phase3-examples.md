# Phase 3: Examples & Use-cases — Store router forwards asset info

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A titled folder through the router | The member's description survives mounting | AC-1, AC-2 |
| 2 | Example | Nested mounts and an overlay | Composition and the union `is_dir` | AC-5, AC-9, AC-10 |
| 3 | Test | `store::tests::routerfwd01_member_file_asset_info_is_forwarded` | File info is the member's | AC-1 |
| 4 | Test | `store::tests::routerfwd02_member_directory_asset_info_is_forwarded` | Directory info is the member's | AC-2 |
| 5 | Test | `store::tests::routerfwd03_directory_above_members_has_directory_asset_info` | Router-built directory info | AC-3 |
| 6 | Test | `store::tests::routerfwd04_unowned_key_asset_info_not_found` | `KeyNotFound` | AC-4 |
| 7 | Test | `store::tests::routerfwd05_listing_composes_children_from_their_owners` | Per-child owners, once each, sorted | AC-5 |
| 8 | Test | `store::tests::routerfwd06_sole_contributor_listing_is_forwarded` | Member's `listdir_asset_info` is called | AC-6 |
| 9 | Test | `store::tests::routerfwd07_contains_above_and_outside_members` | `Ok(true)` / `Ok(false)`, no error | AC-7 |
| 10 | Test | `tests/store_router_STORE.rs::core_router06_asset_manager_describes_a_directory_above_members` | Through `AssetManager` | AC-7 |
| 11 | Test | `tests/store_conformance_CONF.rs::c6_router_with_a_decorated_member_and_a_nested_mount` | Whole suite, no allowed failure | AC-8 |
| 12 | Test | `tests/store_conformance_CONF.rs::c3_async_store_router` (existing) | Still passes | AC-8 |
| 13 | Test | `store::tests::routerfwd09_directory_metadata_children_include_a_nested_mount` | `children` = router listing | AC-9 |
| 14 | Test | `store::tests::routerfwd10_is_dir_asks_every_covering_member` | Overlay first or last, same answers | AC-10 |

## Example 1: A titled folder through the router

A member decorates what it describes — the shape of the front-matter adapter, reduced to a test
store: `DecoratedStore` wraps an `AsyncMemoryStore`, forwards every trait method, and overrides
`get_asset_info` to set `title` to `"Decorated <filename>"`.

```rust
let mut router = AsyncStoreRouter::new();
router.add_store(Box::new(DecoratedStore::new(&parse_key("memory")?)));
router.set(&parse_key("memory/decisions/a.md")?, b"# A", &Metadata::new()).await?;

let file = router.get_asset_info(&parse_key("memory/decisions/a.md")?).await?;
assert_eq!(file.title, "Decorated a.md");                         // AC-1
let dir = router.get_asset_info(&parse_key("memory/decisions")?).await?;
assert_eq!(dir.title, "Decorated decisions");                    // AC-2
assert!(dir.is_dir);
```

Before this design both titles are `""`: the trait default on the router never calls the member.

## Example 2: Nested mounts and an overlay

Only the delta from Example 1. A second member is mounted inside the first; an overlay with an empty
prefix supports one file and is listed **first**.

```rust
router.add_store(Box::new(DecoratedStore::only(&Key::new(), &parse_key("cfg.yaml")?))); // overlay, first
router.add_store(Box::new(DecoratedStore::new(&parse_key("memory")?)));
router.add_store(Box::new(AsyncMemoryStore::new(&parse_key("memory/inbox")?)));

router.is_dir(&parse_key("memory")?).await?;          // true: the overlay says false, the next member true (AC-10)
router.listdir_asset_info(&parse_key("memory")?).await?;
// [inbox (dir, router- or member-built), decisions (dir, "Decorated decisions"), …] — each once (AC-5)
router.get_metadata(&parse_key("memory")?).await?;    // children name `inbox` too (AC-9)
```

## Edge and Error Cases

| Symptom (from Phase 2 risks) | Expected | Test |
|---|---|---|
| Key owned by no member and not above one | `get_asset_info` → `KeyNotFound`; `contains` → `Ok(false)` | `routerfwd04`, `routerfwd07` |
| Relative key | `Err` from `as_absolute`, as every router method today | `routerfwd07` (one assertion) |
| Member listed first answers `false` for a directory another member holds | `is_dir` / `contains` `true`, `listdir` lists it, either member order | `routerfwd10` |
| A member mounted beneath a directory another member owns | `get_metadata` children and `listdir_asset_info` both include the mount, once | `routerfwd05`, `routerfwd09` |
| Existing behaviour of unowned `get_metadata` | Still `KeyNotFound` | existing `router_unowned_key_metadata_not_found` |
| Conformance rules (`dir07` children = listing) on a nested mount | Pass | `c6_…` |

No serialization, persistence, binding or feature-flag case: nothing stored or exposed changes shape.

## Test Plan

- Unit, `liquers-core/src/store.rs` `mod tests`: `routerfwd01`–`routerfwd07`, `routerfwd09`,
  `routerfwd10` as in the table. A test-only `DecoratedStore` (inner `AsyncMemoryStore`, optional
  `only: Option<Key>` narrowing `is_supported` to one key, a call log) supplies member overrides.
  `routerfwd06` asserts the log records the member's `listdir_asset_info` call.
- Integration, `liquers-core/tests/store_router_STORE.rs`: `core_router06_…` builds a
  `SimpleEnvironment` whose store is a router with members at `memory/a` and `memory/b`, and asserts
  `get_asset_info(memory)` through the asset manager is `Ok` with `is_dir`, and `contains` of an
  unowned key is `Ok(false)`.
- Integration, `liquers-core/tests/store_conformance_CONF.rs`: `c6_…` runs the generic suite over a
  router with a decorating member at `mem` and an `AsyncMemoryStore` at `mem/nested`, key base `mem`,
  `C6_ALLOWED = &[]`, and adds a row to `status_table`.
- Command: `cargo test -p liquers-core --lib store::tests` and
  `cargo test -p liquers-core --test store_router_STORE --test store_conformance_CONF`.

## Learning Log

- `AssetManager::get_asset_info` asks `contains` before anything else, so a store's `contains`
  error is a client-visible failure, not an internal one. Phase 5 should say so in the guide.
- The router has two routing rules — "owner" (`find_store`, needs `is_supported`) for data, "every
  covering member" (`listdir`) for directories. This design makes `is_dir` follow the second.
