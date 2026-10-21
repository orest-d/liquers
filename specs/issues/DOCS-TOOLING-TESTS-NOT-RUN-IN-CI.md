---
id: DOCS-TOOLING-TESTS-NOT-RUN-IN-CI
kind: issue
title: The unit tests of the documentation tooling are not run by CI
status: draft
priority: P3
complexity: S
area: [docs, build]
design:
created: 2026-10-20
github:
---

## Problem

`scripts/test_docs_index.py` (49 stdlib `unittest` tests, passing at main 18732dd with
`python3 -m unittest scripts/test_docs_index.py`) is not run by `.github/workflows/docs-check.yml`,
which runs only `python3 scripts/docs_index.py --check`. Its path filter also omits
`scripts/test_docs_index.py`. The `liquers-project` skill scripts (`init_feature.py`,
`validate_phase.py`) have no tests at all. The designs `validate-phase-section-match`,
`phase5-skeleton-unticked` and `front-matter-yaml-agreement` each add tests beside these scripts.

## Impact

A change to `docs_index.py` that breaks a tested behaviour passes CI as long as the current
`specs/` tree still validates. The tests protect only whoever remembers to run them.

## Expected behaviour

The docs workflow also runs `python3 -m unittest scripts/test_docs_index.py` and
`python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'`, and its
path filter includes `scripts/**` and `.claude/skills/liquers-project/scripts/**`. Both commands
are stdlib-only, so the workflow's no-dependency contract holds.

## Discovery

Bulk design of `validate-phase-section-match` and `front-matter-yaml-agreement` (2026-10-20).
