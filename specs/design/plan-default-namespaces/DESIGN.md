---
id: PLAN-DEFAULT-NAMESPACES
kind: design
title: Default namespaces for plan building, held by the environment and set by its configuration
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/plan, core/context, core/commands]
issues: [PLAN-DEFAULT-NAMESPACES-NOT-CONFIGURABLE]
affects_docs: [reference/ENVIRONMENT_CONFIG.md, reference/RECIPES_AND_PLANS.md, guides/ENVIRONMENT_CONSTRUCTION_GUIDE.md]
created: 2026-11-01
---
# Default namespaces for plan building

Triage design (`.claude/skills/liquers-project/references/issue-triage.md`, case 3b), 2026-11-01:
Phases 1-4 written under the bulk-design rules; awaiting review. GitHub #235.

The command metadata registry already holds the namespace chain the planner falls back on
(`default_namespaces`, `["", "root"]`); this design makes it settable **per realm** in the registry
and, for the main realm, from `EnvironmentConfig` (`default_namespaces:`), which takes precedence.
Lookup stays a first-match chain: the namespaces of the last `ns` before the action, then the
realm's defaults. `Environment::default_namespaces(realm)` reports the chain.

Revised 2026-11-01 after the user's review: an explicit `ns` **extends** the defaults (it is
searched first, then the defaults) rather than replacing them; defaults are per realm in the
registry; the configuration covers the main realm only.

## Phases

1. [High-level design](phase1-high-level-design.md)
2. [Architecture](phase2-architecture.md)
3. [Examples and tests](phase3-examples.md)
4. [Implementation plan](phase4-implementation.md)
