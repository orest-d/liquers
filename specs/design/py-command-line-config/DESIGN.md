---
id: PY-COMMAND-LINE-CONFIG
kind: design
title: Start a Python Liquers application from a configuration file with command-line overrides
workflow: liquers-project
readiness: ready
autofix: not-eligible
gh_pr: [273]
phase: documentation
area: [py, docs, core/context]
issues: [PY-APPLICATION-CANNOT-START-FROM-A-CONFIG-FILE]
affects_docs: [specs/reference/ENVIRONMENT_CONFIG.md, specs/guides/PYTHON_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, docs/guides/python/08-environment.md, docs/guides/python/11-ui.md, docs/guides/python/12-serving.md]
created: 2026-11-14
---
# Start a Python Liquers application from a configuration file with command-line overrides

Split from [`py-default-environment-config`](../py-default-environment-config/DESIGN.md) on
2026-11-14 to keep both within the size limits, and built on its `configure`. Phase 1 was written
and reviewed twice on 2026-11-14 (conformity and records, then codebase and soundness) while it
was still part of that design. Phase 1 approved 2026-11-15 (Orest: "proceed", accepting both
recommendations below, with a gate after each phase). Phase 2 written 2026-11-15.

Converted from the compact to the full form on 2026-11-15, when Phases 3 and 4 took it past the
compact size limit. The source issue was re-sized from `M` to `L`, because the design changes
`liquers-core` as well as `liquers-py`. Pre-approved after Phase 2 on 2026-11-15 (Orest:
"proceed all"). Phases 3 and 4 written 2026-11-15. Implemented and Phase 5 written 2026-11-15 on PR #273.

## Phases

- [Phase 1](./phase1-high-level-design.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
