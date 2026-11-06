---
id: LIQUERS-PY-MODULES
kind: design
title: "Modules from the store in liquers-py: import Python code kept beside its recipes"
workflow: liquers-project
status: in_review
phase: high-level
readiness: needs-decision
autofix: not-eligible
area: [py]
issues: [PY-CANNOT-IMPORT-MODULES-FROM-THE-STORE]
affects_docs: [specs/reference/PYTHON_INTEGRATION.md, specs/guides/PYTHON_GUIDE.md, specs/guides/LANGUAGE-INTEGRATION_GUIDE.md, liquers-py/README.md, specs/README.md]
created: 2026-11-06
---
# liquers-py-modules — import Python code kept in the store

The `MODULE` feature for liquers-py. Planned for milestone M7, and split out of it on 2026-11-06 at
Orest's request ("Let's skip the modules support for now - make a separate issue and design about
that"): it waits here, at the Phase 1 gate, until it is taken up again. Full form (`L`). Requires `lq.link` from `liquers-py-templates` (M6, PR #262).
Based on `main`; M6 (PRs #257, #259–#263) lands first. The M7 server and UI designs are in PR #264.

Phase 1 written and reviewed twice on 2026-11-06 (conformity and records; codebase and soundness); findings applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
