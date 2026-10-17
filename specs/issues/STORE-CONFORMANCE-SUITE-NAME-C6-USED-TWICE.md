---
id: STORE-CONFORMANCE-SUITE-NAME-C6-USED-TWICE
kind: issue
title: Conformance suite name C6 names two different suites
status: closed
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

## Resolution (2026-10-16)

Fixed in PR #137: `liquers-core`'s router suite is now `C8` (`c8_router_with_a_decorated_member_and_a_nested_mount`); `C6`–`C7` stay `liquers-store`'s OpenDAL suites. `STORE_IMPLEMENTATION_GUIDE.md` §7 and `STORE_SEMANTICS.md` §6 follow.
