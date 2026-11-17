---
id: PY-CANNOT-INSPECT-THE-DEPENDENCY-MANAGER
kind: feature
title: Python cannot inspect the dependency manager
status: draft
priority: P3
complexity: M
area: [py]
design: 
created: 2026-11-14
github:
---
## Problem

`liquers_core::dependencies::DependencyManager` (`liquers-core/src/dependencies.rs`) tracks which
assets depend on which keys, commands and recipes, and expires dependents when one changes. The
Python package wraps only its value types (`DependencyRelation`, `PlanDependency` in
`liquers-py/src/dependencies.rs`; `DependencyKey`, `DependencyRecord` in `liquers-py/src/metadata.rs`).
No Python object reaches the manager itself, so Python cannot ask what depends on a key, what a key
depends on, or which version the manager holds.

## Impact

Debugging stale or unexpectedly expired assets from Python requires reading metadata by hand. No
workaround through the public API. Low urgency: no current Python feature needs it.

## Expected behaviour

A read-only wrapper reachable from an `Environment` (for example `env.dependencies`), exporting
lookups of dependents and dependencies of a `DependencyKey`, and the held version (see design
`dependency-manager-registered-version`). It belongs in the planned `liquers.dependencies`
submodule (`py-package-submodules`).

## Discovery

Raised by Orest on 2026-11-14 at the Phase 1 gate of `py-package-submodules` ("is there no wrapper
for dependency manager? That would deserve its own module").
