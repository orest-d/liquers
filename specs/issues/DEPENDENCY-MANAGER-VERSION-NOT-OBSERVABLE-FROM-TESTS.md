---
id: DEPENDENCY-MANAGER-VERSION-NOT-OBSERVABLE-FROM-TESTS
kind: issue
title: An integration test cannot ask the dependency manager whether a key has a registered version
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-10-17
github:
---
## Problem

`DependencyManager::get_version` (`liquers-core/src/dependencies.rs`) is `pub(crate)`, and no public
method answers "is this key a graph node, and with which version". An integration test that must
show an asset was *not* registered — `design/runtime-volatility` AC-3, "the key registers no
version in the dependency graph" — can only assert the proxy that the asset's own metadata has no
version.

## Impact

Graph-registration claims in integration tests are checked indirectly, so a regression that
registers a version from somewhere else would pass.

## Expected behaviour

A read-only public accessor (for instance `DependencyManager::registered_version(&DependencyKey)
-> Option<Version>`), or a test-support feature that exposes `get_version`.

## Discovery

`liquers-core/tests/runtime_volatility.rs` `rv03_*`, while implementing `design/runtime-volatility`.
