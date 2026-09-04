---
id: ACTIVE-02
kind: design
title: Design for CORE-ERROR-STORE-NAME-NOT-STRUCTURED
status: abandoned
area: [core/error, core/store]
issues: [CORE-ERROR-STORE-NAME-NOT-STRUCTURED]
created: 2026-09-03
---

# Design Tracking

## Resolution

Abandoned on 2026-09-04 with its source issue. A store name embedded in the error message is
sufficient for now. Any future structured store reference belongs to the broader
[`ERROR-WITH-KEY-SETS-QUERY-FIELD`](../../issues/ERROR-WITH-KEY-SETS-QUERY-FIELD.md) design, where
it can be modeled with the rest of diagnostic context rather than as a competing payload field.

- [x] Phase 1: High-Level Design
- [x] Phase 2: Architecture
- [x] Phase 3: Examples and Tests
- [x] Phase 4: Implementation Plan

