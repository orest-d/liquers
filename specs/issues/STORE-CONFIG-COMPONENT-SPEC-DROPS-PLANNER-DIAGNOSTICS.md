---
id: STORE-CONFIG-COMPONENT-SPEC-DROPS-PLANNER-DIAGNOSTICS
kind: issue
title: A storage or writes spec loses the planner's info and warning steps
status: draft
priority: P3
complexity: S
area: [core/store, store/config]
design:
created: 2026-10-15
github:
---
## Problem

`StoreConfigFunctions::compile_component` (`liquers-core/src/store/realm.rs`) skips the plan's
`Info`, `Warning` and `Error` steps, whereas `compile` turns them into steps that write messages
into the record. So a warning the planner attaches to a `storage:` or `writes:` spec is never seen.

## Impact

Small: a component spec names one entry, so few warnings arise, but a deprecation or similar note
on a storage model would be silent.

## Expected behaviour

A planner `Error` step fails the build; `Info` and `Warning` steps are reported with the build
warnings (see `STORE-BUILD-WARNINGS-LOST-IN-THE-BROWSER`).

## Discovery

Found while documenting `design/store-adapters/` (Phase 5).
