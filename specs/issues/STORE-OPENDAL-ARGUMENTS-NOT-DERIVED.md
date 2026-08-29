---
id: STORE-OPENDAL-ARGUMENTS-NOT-DERIVED
kind: feature
title: OpenDAL store types have no derived argument descriptions and no offline construction test
status: draft
priority: P3
complexity: S
area: [store/backends, store/config]
design: store-factories-in-core
created: 2026-08-29
github:
---
## Problem

This is the **remainder of [`design/store-factories-in-core/`](../design/store-factories-in-core/)**
(§5.6): two steps of its approved plan were deferred rather than executed, and that design is now
`complete` and its folder frozen. Both are blocked by the same prerequisite.

**Step 9 — derived argument descriptions.** `OpendalStoreFactory::common_arguments`
(`liquers-store/src/store_factory.rs`) hand-writes a handful of arguments per store type. The plan
was to *derive* the full list from the linked OpenDAL instead, so it cannot go stale:
`Configurator` bounds `Serialize`, every service config derives `Default`, and none carries
`skip_serializing_if`, so `serde_json::to_value(C::default())` yields every field name with its
default. `StoreArgumentInfo::derived` exists in `liquers-core` for exactly this and currently has
one user, a round-trip test.

**Step 11 — offline S3 tests.** `s3_01_arguments_and_uri_agree` and
`s3_02_missing_region_fails_at_construction`, specified in that design's Phase 3 §"The offline S3
test" and verified by hand against OpenDAL 0.55 during design. They need no credentials and no
network, because OpenDAL builders are lazy.

## Impact

Low, hence P3. The types are still described — `ArgumentCoverage::Partial` says the hand-written
list is guidance and names OpenDAL's documentation as the authority, so nothing claims to be
complete and is not. The missing tests cover a path that other tests reach in shape if not in
service.

What is actually lost: the argument descriptions a user or coding agent sees for an OpenDAL type
are a handful rather than the real set, and they will drift from OpenDAL's on any upgrade —
precisely the drift derivation was designed to make impossible.

## Prerequisite

**[`STORE-OPENDAL-SERVICES-NOT-ENABLED`](STORE-OPENDAL-SERVICES-NOT-ENABLED.md) (P0) blocks both**,
for two different reasons.

Deriving requires *naming* a config type, and `opendal::services::S3Config` is behind
`#[cfg(feature = "services-s3")]` (`opendal-0.55.0/src/services/mod.rs`). With no service features
enabled, `cargo check` fails with `cannot find type FsConfig in module opendal::services`, and the
only nameable config is `MemoryConfig` — not an `OPENDAL_STORE_TYPES` entry. This was attempted
during implementation and reverted; the reasoning is in `common_arguments`'s doc comment.

The S3 tests need `services-s3` for the ordinary reason: without it, `s3` is not constructible at
all.

## Expected behaviour

Once the P0 is fixed:

1. Add `derived_arguments<C: Configurator + Default>()` and a `store_type -> config type` match over
   the services the build enables. Merge hand-written `doc` text onto the derived entries by name,
   so only *documentation* stays ours.
2. Add the two S3 tests from that design's Phase 3.

**One trap, stated because it would quietly undo the point.** `derive01` must assert that a few
long-stable field names are **present** (`bucket`, `region`, `root`), never that the list is
exhaustive. An exhaustive assertion reintroduces through the test suite exactly the maintenance
burden derivation removes: it would fail on every OpenDAL release that adds a field.

The `store_type -> config type` match is the only hand-maintained part, and it changes when a
*service* is added or removed — the same cadence as `OPENDAL_STORE_TYPES`, which is hand-maintained
already. Forgetting an entry degrades to "no arguments reported", which under `Partial` is honest
rather than wrong.

## Discovery

Filed when `design/store-factories-in-core/` reached `complete` with two plan steps unexecuted.
Recorded as an issue rather than left inside the design, because a `complete` design's folder is
frozen (§5.1) and there is no partial design status (§5.6).
