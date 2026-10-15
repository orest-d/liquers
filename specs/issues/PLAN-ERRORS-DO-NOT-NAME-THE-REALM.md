---
id: PLAN-ERRORS-DO-NOT-NAME-THE-REALM
kind: issue
title: A command not found in a realm is reported without the realm, and mixed-realm queries plan without a warning
status: draft
priority: P3
complexity: S
area: [core/plan, core/commands]
design:
created: 2026-10-15
github:
---
## Problem

Two gaps, both visible since the planner applies a segment's realm to every action
(`design/store-adapters/` step 0):

- `-foo/to_text` fails with "Action 'to_text' not registered in namespaces …", although `to_text`
  exists in realm `""`; the message does not say which realm was searched.
- A query mixing realms, such as `-store_config/defaults/-/to_text`, plans with no warning and fails
  only when executed, because `store_config` is not executable.

## Impact

Confusing messages for anyone writing realm-prefixed queries, and a validation tool
(`liquers-validate`) that reports such a query as fine.

## Expected behaviour

The not-found message names the realm (and suggests the realm where the command does exist), and
the planner warns when one query reaches both an executable and a non-executable realm.

## Discovery

Found while writing `specs/reference/REALMS.md` (store-adapters Phase 5).
