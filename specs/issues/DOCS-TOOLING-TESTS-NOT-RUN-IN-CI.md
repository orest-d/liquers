---
id: DOCS-TOOLING-TESTS-NOT-RUN-IN-CI
kind: issue
title: The unit tests of the documentation tooling are not run by CI
status: draft
priority: P3
complexity: S
area: [docs, build]
design: docs-tooling-tests-in-ci
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

## Problem Example

Suppose a PR reintroduces the fenced-heading bug that `PhaseTextTests` in
`scripts/test_docs_index.py` catches. The `docs` job runs only `--check`, which passes because no
current design has a fenced `## Phase` heading, so the PR merges green. A PR that edits only
`scripts/test_docs_index.py` does not run the workflow at all. Also, the push filter omits the
workflow file. Note for the fix: `python3 -m unittest discover -s .claude/skills/liquers-project/scripts
-p 'test_*.py'` with no test files prints `NO TESTS RAN` and **exits 5 on Python 3.12 and later**
(verified on 3.13; the workflow pins 3.11, where it exits 0). The step needs a presence guard.
Re-checked at `cd6d5da`.

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
