---
id: ASSET-MANAGER-TRAIT-CANNOT-BE-IMPLEMENTED-OUTSIDE-CORE
kind: issue
title: The AssetManager trait is sealed, so a custom asset manager cannot be written outside liquers-core
status: draft
priority: P2
complexity: L
area: [core/assets]
design: dependency-audit-and-expiry-provenance
created: 2026-09-28
github:
---

## Problem

`AssetManager<E>` (`liquers-core/src/assets.rs:3895`) has a supertrait,
`DependencyManagerAccess<E>`, that is `pub(crate)` (`assets.rs:3871`). Its one method returns the
equally crate-private `DependencyManager<E>`. A crate outside `liquers-core` can neither name the
supertrait nor implement it, so it cannot implement `AssetManager` at all. `Environment::AssetManager`
is therefore always one of the two built-in managers, `DefaultAssetManager` and `ImmediateAssetManager`.

The sealing was deliberate: `keyed-expiry-cascade-fix` relied on it so that it could add a
supertrait freely (`design/keyed-expiry-cascade-fix/DESIGN.md`, "already sealed"). It keeps the
dependency graph out of the supported API. `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` lists the related
visibility mismatch as a P1 API finding.

## Impact

The project owner expects that a custom asset manager may be needed outside core (2026-09-28, at
the Phase 2 gate of `dependency-audit-and-expiry-provenance`). Examples are a manager backed by a
remote job system or a database, or one with its own eviction policy. Today that requires forking
`liquers-core`. No workaround exists within the published API.

## Expected behaviour

Decide what an external manager must provide, and unseal on purpose. Either:

1. split `AssetManager` into a public, implementable surface and an internal layer that the
   built-in managers share, supplied through a public helper (for example a public
   `DependencyManager` with a documented, narrow API); or
2. keep it sealed and document that as a supported restriction, recording why.

Unsealing must also cover the default methods that currently reach crate-private items (audit,
cascade and version resolution), and it should come with a conformance suite similar to the store
conformance suite. A trait that anyone can implement needs a check that an implementation behaves
correctly.

## Discovery

Found 2026-09-28 while checking whether `#[non_exhaustive]` on `AuditReport` would stop a custom
asset manager from building one (`design/dependency-audit-and-expiry-provenance/`, Phase 2 gate).
It would not, but a custom manager cannot exist in the first place.
