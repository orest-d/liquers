---
id: DELEGATED-VALUE-REPERSISTED
kind: issue
title: An asset that delegates to a key's owner writes the owner's value to the store again
status: closed
priority: P3
complexity: S
area: [core/assets]
design: delegated-value-repersisted
created: 2026-08-12
github:
---

## Problem

When `AssetRef::evaluate_recipe` takes the delegation branch, it returns the *owner's* state
(`liquers-core/src/assets.rs`, the `Some(asset) if asset.id() != self.id()` arm). The caller,
`evaluate_and_store`, cannot tell that state apart from one it computed itself: it installs the
value, calls `try_to_set_ready`, and then `persist_with_status_tracking`. `save_to_store` resolves
the target from `lock.recipe.key()`, which is the same key the owner persisted under, so the same
bytes and the same metadata are written to the store a second time.

## Impact

Low. The second write is idempotent — the content is the owner's, unchanged — so no value is
corrupted and no reader observes anything wrong. What it costs is a redundant store round-trip per
delegating asset, which for a remote backend (OpenDAL S3, HDFS) is a real request rather than a
memcpy, and a redundant serialization when the binary is not already cached.

There is no correctness hazard from concurrency either: the delegating asset writes only after
`wait_for_dependency` has returned the owner's finished value, so the two writes are ordered and
carry identical content.

`DependencyManager::track_asset` already avoids the analogous mistake — it resolves the key through
`bound_owner_key()`, which returns `None` for a non-owner, so the delegating asset does not
re-register a version. Persistence has no equivalent check.

## Expected behaviour

A delegating asset should not persist. It did not produce the value and it does not own the key;
the owner is responsible for both. Either:

1. **Flag the state as delegated** so `evaluate_and_store` skips persistence for it — needs a way
   to carry that fact out of `evaluate_recipe`, which currently returns a bare `State`.
2. **Or gate persistence on ownership**, the way `track_asset` does: skip the store write when
   `bound_owner_key()` is `None` *and* the recipe key names an asset owned by someone else. Cheaper,
   but it changes behaviour for every non-owning keyed asset, not only delegating ones, so it needs
   checking against `Context::set_state` and the ad-hoc `apply` paths that legitimately write.

(2) is the smaller change; (1) is the more precise one.

## Verification

A store counting `set` calls, wrapped around the `keyed_delegation_{default,immediate}` arrangement
in `liquers-core/tests/manager_parametric.rs`: the key should be written once, by the owner, not
twice. The existing call-count assertion on the recipe stays as is.

## Discovery

Found on 2026-08-12 while fixing `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`
(`specs/design/keyed-delegation-hand-off/`). Before that fix the delegation branch always errored,
so it never reached persistence and this was unreachable. Deliberately left out of scope there: it
is a property of `evaluate_and_store`, not of the dependency-cycle check that issue was about.

## Resolution

Closed on 2026-09-01. Recipe evaluation now carries a private delegated outcome to
`evaluate_and_store`, which installs and readies the handed-off value but skips the delegating
asset's persistence attempt. Counting-store tests cover both default and immediate managers.
