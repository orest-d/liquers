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

An environment names namespaces that every query searches as if it began with an implicit `ns`
instruction. They are set by `default_namespaces:` in `EnvironmentConfig` or by
`EnvironmentBuilder::with_default_namespaces`, kept on the command metadata registry so every
resolver sees them, and reported by `Environment::default_namespaces()`. Unset, nothing changes:
only root is searched.

## Phases

1. [High-level design](phase1-high-level-design.md)
2. [Architecture](phase2-architecture.md)
3. [Examples and tests](phase3-examples.md)
4. [Implementation plan](phase4-implementation.md)
