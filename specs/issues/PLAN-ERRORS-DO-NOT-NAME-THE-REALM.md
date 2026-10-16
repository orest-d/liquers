---
id: PLAN-ERRORS-DO-NOT-NAME-THE-REALM
kind: issue
title: A command not found in a realm is reported without the realm, and mixed-realm queries plan without a warning
status: closed
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

## Resolution (2026-10-16)

The first half is fixed in PR #137, at the user's request: `Error::action_not_registered` takes the
realm and names every `CommandKey` tried, in its `Display` form (`-foo/to_text` reports
`'foo--to_text'`); test `action_not_registered_names_every_command_key_tried`.
The second half (mixed-realm queries plan without a warning) is not planned for now: inside a store
configuration it cannot arise, because the realm is filled into every segment, so it affects only
hand-written queries in `liquers-validate`.
