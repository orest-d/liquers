---
id: AGENT-COMMAND-PROPOSAL
kind: design
title: An agent proposes alias commands into a staging realm, for a person to promote
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [mcp, core/commands, docs]
issues: [AGENT-CANNOT-PROPOSE-A-COMMAND]
affects_docs: [specs/reference/AGENT_MEMORY_SERVICE.md, specs/reference/COMMAND_ALIASES.md, specs/reference/REALMS.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-10-27
---
# An agent proposes alias commands into a staging realm

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-27: Phases 1-4 written and
reviewed without phase approval. Nothing is implemented. First slice of W3 in
`design/agent-memory-mvp/workbench-direction.md`: alias proposals only, no Python.

It cannot be built before `post-init-command-registration` (run-time registration into a shared
environment), whose trait-shape decision is carried here as an assumption.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
