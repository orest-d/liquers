---
id: WEB-API-SPEC-SAYS-A-DIRECTORY-MOUNT-CANNOT-BE-READ-ONLY
kind: issue
title: The web API specification says a directory mount stays writable, but mounts can be read-only
status: draft
priority: P3
complexity: S
area: [docs, axum]
design:
created: 2026-10-20
github:
---

## Problem

`specs/reference/WEB_API_SPECIFICATION.md` §9 says: "A store mounted from a directory stays
writable through the Store API's `PUT`/`DELETE` routes and the Assets API's mutations unless those
are left out (`STORE-NO-READ-ONLY-ADAPTER`)." That issue is closed: `liquers-core` has
`ReadOnlyStore` (`liquers-core/src/store/read_only.rs`) and store configuration has `read_only`
(`store_config.rs`), so a mount can refuse writes whatever routes are exposed.

## Impact

A reader securing a deployment is told to remove routes when a configuration flag would do; the
reference document is not true at HEAD.

## Expected behaviour

§9 names the `read_only` mount setting (and `ReadOnlyStore`) as the way to protect a mounted
directory, and what a write to it answers through the APIs; History row and `reviewed:` bump.

## Discovery

Found 2026-10-20 while designing `access-policy-at-entry-points` (bulk design), which rewrites §9
later; this correction does not need to wait for it.
