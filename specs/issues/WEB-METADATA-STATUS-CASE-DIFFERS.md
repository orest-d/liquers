---
id: WEB-METADATA-STATUS-CASE-DIFFERS
kind: issue
title: State.metadata().status is capitalized while Asset.status() and State.status are lowercase
status: draft
priority: P3
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`Asset.status()`, `State.status` and the `status` of a `state: "state"` command argument use
`status_name` (`liquers-web/src/asset.rs`): `"ready"`, `"error"`, …. `State.metadata()` and that
argument's `metadata` are the `serde` form of `MetadataRecord`, whose `status` is the variant name:
`"Ready"`. Measured on a wasm build (2026-10-26).

## Impact

A page that compares `metadata.status === "ready"` is always false. Cosmetic otherwise.

## Expected behaviour

One spelling of a status across the module, most simply by normalizing `status` in
`metadata_to_js`.

## Discovery

Writing `specs/guides/JAVASCRIPT_GUIDE.md` and checking its examples against a build (2026-10-26).
