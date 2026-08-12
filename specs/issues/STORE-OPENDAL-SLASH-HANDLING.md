---
id: STORE-OPENDAL-SLASH-HANDLING
kind: issue
title: OpenDAL store mishandles keys containing slashes
status: accepted
priority: P1
complexity: M
area: [store/backends]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-store/src/opendal_store.rs:335` carries
`// FIXME: This currently does not work due to some bug with handling '/'`. Directory creation is
also stubbed at `:110`, `:119`, `:357` and `:374` (`//TODO: create_dir`).

## Impact

Keys that contain a `/` — which is to say most real keys — are not reliably addressable through an
OpenDAL-backed store. This is a correctness bug against real backends, not a limitation.

## Expected behaviour

Path normalization is applied in one place, with a round-trip property test: any `Key` encoded to a
backend path and decoded returns the original. WP-5 proposes a dedicated `path_map.rs` and strict
rewrites in the store tests.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #6, work package WP-5. Verified against HEAD: the FIXME is still at `opendal_store.rs:335`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
