---
id: STORE-CONFORMANCE-SUITE-NAME-C6-USED-TWICE
kind: issue
title: Conformance suite name C6 names two different suites
status: draft
priority: P3
complexity: S
area: [core/store, store/backends, docs]
design:
created: 2026-10-15
github:
---
## Problem

`C6` is the nested-router suite in `liquers-core/tests/store_conformance_CONF.rs` and the OpenDAL
memory suite in `liquers-store/tests/store_conformance_CONF.rs`. The status table in
`specs/guides/STORE_IMPLEMENTATION_GUIDE.md` §9 therefore cannot name a suite unambiguously.

## Expected behaviour

One suite keeps `C6`, the other is renumbered, and the guide's table follows.

## Discovery

Found while documenting `design/store-adapters/` (Phase 5).
