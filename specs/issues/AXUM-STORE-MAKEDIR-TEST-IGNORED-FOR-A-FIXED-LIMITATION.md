---
id: AXUM-STORE-MAKEDIR-TEST-IGNORED-FOR-A-FIXED-LIMITATION
kind: issue
title: An axum store test is ignored for an AsyncMemoryStore limitation that has been fixed
status: draft
priority: P3
complexity: S
area: [axum, core/store]
design: axum-store-makedir-test-unignore
created: 2026-10-10
github:
---
## Problem

`liquers-axum/tests/store_api_integration.rs` `test_store_makedir` (≈208-218) is `#[ignore]`d with
the note "Ignored because MemoryStore doesn't support directory operations". The store it builds is
`AsyncMemoryStore` (`create_test_store`, ≈33), and `AsyncMemoryStore::makedir` was made to create a
real directory entry by `CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING` (closed, via
`design/opendal-path-mapping/`). The reason no longer holds, so a passing test is not being run.

Not yet confirmed by running it with `--ignored`; that run is the first step of the fix.

## Impact

Lost coverage only: `makedir` through the axum test fixture is never exercised. No user-facing
behaviour is wrong. The sibling tests `test_store_is_dir` / `test_store_listdir` call `makedir` but
discard its result, so they do not cover it either.

## Expected behaviour

Run `cargo test -p liquers-axum --test store_api_integration -- --ignored test_store_makedir`. If it
passes, remove `#[ignore]` and the stale note; if it fails, replace the note with the actual reason
and file that failure.

## Discovery

Found 2026-10-10 while searching for references to the synchronous `MemoryStore` for
`design/sync-store-removal/` (`CORE-SYNC-STORE-TRAIT-OBSOLETE`): the note names the removed sync type
but the fixture uses `AsyncMemoryStore`. Triage: no open candidate (the matching makedir issues are
closed); eligible for automatic fixing (size `S`, tests only, no interface change). Not fixed in the
sync-store-removal branch, which must not widen; filed for its own branch.
