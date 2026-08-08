---
id: CORE-MULTI-REALM-INTERPRETER
kind: issue
title: The interpreter supports a single realm
status: draft
priority: P3
complexity: XL
area: [core/plan, core/commands]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/plan.rs:1081` — `// TODO: RQS realm should should be supported`. Command keys
carry a realm, but the interpreter does not dispatch across more than one.

## Impact

Strategic rather than immediate: a realm is the mechanism by which one deployment would host
several independent command sets, and it does not work.

## Expected behaviour

Realm-aware dispatch, with the realm participating in command resolution and in the plan.

Wants a design, and it should follow the evaluation-path consolidation rather than precede it.

## Discovery

Migration triage, 2026-08-08. Source: work packages WP-18/19. Verified against HEAD: marker present at `plan.rs:1081`. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
