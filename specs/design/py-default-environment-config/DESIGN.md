---
id: PY-DEFAULT-ENVIRONMENT-CONFIG
kind: design
title: Configure the Python default environment after commands are registered
workflow: liquers-project
gh_pr: [273]
phase: high-level
area: [py, docs]
issues: [PY-DEFAULT-ENVIRONMENT-CANNOT-BE-CONFIGURED]
affects_docs: [specs/guides/PYTHON_GUIDE.md, specs/reference/PYTHON_INTEGRATION.md, docs/guides/python/02-installing.md, docs/guides/python/08-environment.md, docs/guides/python/12-serving.md]
created: 2026-11-14
---
# py-default-environment-config: configure an environment after its commands are registered

`lq.configure(...)` and `env.configure(...)` set an environment's settings at any point before its
first successful build, whatever order modules registered their commands in. Starting from a
configuration file with command-line overrides is the dependent design
[`py-command-line-config`](../py-command-line-config/DESIGN.md). The design lives in `liquers-py` and its documentation.
It is based on `main` at `2c1a94e`.

Phase 1 was written on 2026-11-14 as a compact design and reviewed twice: conformity and records,
then codebase and soundness. The findings are applied. At the Phase 1 gate on 2026-11-14, Orest
accepted open questions 1 and 2. He also asked for a command-line start-up and YAML-configured UI
and server examples, which were split into `py-command-line-config` to keep within the size
limits. This design was re-sized to `L` and converted to the full form. The revision was reviewed
twice again, and the findings are applied.

## Phases

- [Phase 1](./phase1-high-level-design.md)
