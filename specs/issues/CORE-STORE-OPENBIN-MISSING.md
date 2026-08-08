---
id: CORE-STORE-OPENBIN-MISSING
kind: issue
title: `openbin` is unimplemented in every store
status: draft
priority: P3
complexity: M
area: [core/store, store/backends]
design: 
created: 2026-08-08
github:
---
## Problem

`openbin` — streaming access to a stored binary rather than a full read into memory — is stubbed
everywhere: `liquers-core/src/store.rs:207`, `:450`, `:1983` and
`liquers-store/src/opendal_store.rs:498` all carry `// TODO: implement openbin`.

## Impact

Every binary read loads the whole object into memory. Large assets are bounded by RAM, and an HTTP
range request cannot be served without reading the entire object first.

## Expected behaviour

`openbin` returns a reader (or an async stream) that pulls incrementally, with the OpenDAL backend
mapping onto its native ranged reads. WP-14 in the archived implementation plan sketches this and
depends on the OpenDAL path work.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #5, work package WP-14. Verified against HEAD: four markers still present. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
