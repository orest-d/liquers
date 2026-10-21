---
id: POST-INIT-COMMAND-REGISTRATION-DESIGN
kind: design
title: Registering commands into a shared environment
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/commands, core/context]
issues: [POST-INIT-COMMAND-REGISTRATION]
affects_docs: [specs/reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, specs/reference/ASSETS.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-20
---
# Registering commands into a shared environment

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. The source issue was re-sized from `M` to
`L`: the change alters two `Environment` trait signatures and reaches call sites in nine crates.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
