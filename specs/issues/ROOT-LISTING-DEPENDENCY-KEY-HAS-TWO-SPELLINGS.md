---
id: ROOT-LISTING-DEPENDENCY-KEY-HAS-TWO-SPELLINGS
kind: issue
title: The root directory listing dependency has two spellings, so a dependent that recorded one is never expired
status: draft
priority: P3
complexity: S
area: [core/assets]
design: 
created: 2026-10-17
github:
---
## Problem

The root directory's listing is one dependency with two spellings. `DependencyKey::from_dir_key`
(`liquers-core/src/metadata.rs`) formats `-R-dir/{key}`, which for the root key gives `-R-dir/`.
`Step::GetAssetDirectory` (`liquers-core/src/interpreter.rs`) records and registers the root
listing version under that spelling, and `AssetManager::refresh_listing_version` refreshes it under
the same one. A dependency on the query `-R-dir` (a link, `Context::get_dependency_state`, a cut
`Evaluate` boundary) is recorded as `DependencyKey::from(&Query)`, the query's encoding, which is
`-R-dir`. `-R-dir/` is not a parseable query (`liquers-validate -- '-R-dir/'` fails; `-R-dir`
parses), so `DependencyKey::to_query` fails on it too. `is_dir_key` and `dir_key` accept both
spellings, but the dependency manager keys its maps by the string, so the two are different nodes.

A dependent that recorded `-R-dir` is therefore never expired when a file is added at the root of
the store. Non-root folders do not have the problem: `-R-dir/a/b` is both the query encoding and
the `from_dir_key` spelling. `From<&Key> for DependencyKey` has the same shape for the root key
(`-R/`), although a root `-R/` dependency has no practical use.

## Impact

Small: only listings of the store root. Found through `ns-search/catalog` with no input, which
lists the whole store; `design/search-read-dependencies` is not affected, since
`Context::record_listing` records the `from_dir_key` spelling directly. Any other dependent of the query `-R-dir` stays stale after a root-level write.

## Expected behaviour

One spelling per node. Recommended: `from_dir_key(&Key::new())` returns `-R-dir` (and
`From<&Key>` returns `-R` for the root), which is also the query encoding, so `to_query` works and
every recording path agrees. Stored records spelled `-R-dir/` stay readable through `dir_key`, and
are replaced at the next recomputation.

## Discovery

Prototyping `design/search-read-dependencies` (2026-10-17): with the listing read through
`Context::get_dependency_state(-R-dir)`, a root catalog stayed stale after a write at the root while
`-R-dir/specs/issues` invalidated correctly. Related, closed:
`DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`, whose fix did not consider the root.
