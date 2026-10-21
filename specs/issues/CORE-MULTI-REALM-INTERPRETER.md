---
id: CORE-MULTI-REALM-INTERPRETER
kind: issue
title: The interpreter supports a single realm
status: accepted
priority: P3
complexity: XL
area: [core/plan, core/commands]
design: multi-realm-dispatch
created: 2026-08-08
github:
---
## Problem

> **Re-scoped 2026-10-20 (bulk design, `design/multi-realm-dispatch/`).** Partly delivered. The
> realm now takes part in command resolution and in the plan: `PlanBuilder` applies a transform
> segment's realm to every action in it (`store-adapters` Phase 4 step 0; tests
> `realm_header_applies_to_every_action` and siblings in `plan.rs`), `Step::Action` carries it, and
> `CommandRegistry` executes any realm registered locally because it looks executors up by the
> full `CommandKey`. The marker quoted below moved to `plan.rs` ~1536, above
> `process_resource_query`, and concerns a *resource* segment's header name, which is reserved.
> **What remains:** nothing binds a realm to an executor other than the local registry — another
> environment, or another host such as a server called from the browser. The original text
> follows as history.

`liquers-core/src/plan.rs:1081` — `// TODO: RQS realm should should be supported`. Command keys
carry a realm, but the interpreter does not dispatch across more than one.

## Impact

Strategic rather than immediate: a realm is the mechanism by which one deployment would host
several independent command sets, and it does not work.

## Expected behaviour

Realm-aware dispatch, with the realm participating in command resolution and in the plan.

Wants a design, and it should follow the evaluation-path consolidation rather than precede it.

## Discovery

Migration triage, 2026-08-08. Source: work packages WP-18/19. Verified against HEAD: marker present at `plan.rs:1081`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
