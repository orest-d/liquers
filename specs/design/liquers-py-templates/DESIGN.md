---
id: LIQUERS-PY-TEMPLATES
kind: design
title: "Templates in liquers-py: template commands and rendering from scripts and commands"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py, lib/commands]
issues: [PY-CANNOT-RENDER-TEMPLATES]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md, specs/guides/TEMPLATE_GUIDE.md, specs/guides/COMMAND_REGISTRATION_GUIDE.md]
created: 2026-11-04
---
# liquers-py-templates — template commands and rendering from scripts and commands

The `TEMPLATE` feature for liquers-py: milestone M6 of the liquers-py plan, together with
[`liquers-py-data-nodes`](../liquers-py-data-nodes/DESIGN.md) and
[`liquers-py-pipelines`](../liquers-py-pipelines/DESIGN.md). Full form (`L`). Requires a core
primitive that runs a closure as an ad-hoc asset (`AD-HOC-ASSET-CANNOT-RUN-A-CLOSURE`) and
`template-render-in-env` re-based on it (Phase 1 Decision 1), each in its own PR first.

Phase 1 written and reviewed twice on 2026-11-04. At the gate Orest decided question 1 (the top level
renders as a command does); revised, reviewed again, and waiting at the Phase 1 gate.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
