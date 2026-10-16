---
id: STORE-CONFIG-COMPONENT-SPEC-DROPS-PLANNER-DIAGNOSTICS
kind: issue
title: A storage or writes spec loses the planner's info and warning steps
status: closed
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

## Resolution (2026-10-16)

Fixed in PR #137 (store-adapters Phase 5 review). `StoreConfigFunctions::compile_component` now reads the plan's `init_steps` and `steps`: a `Warning` is appended to the caller's notes, which the store builder returns as build warnings naming the mount (`StoreRouterBuilder::build_store_reporting`); an `Error` fails the build; an `Info` (volatility, expiry and similar plan facts) is dropped, since a component has no record to carry it. Test `component_spec_reports_planner_warnings_and_errors`.
