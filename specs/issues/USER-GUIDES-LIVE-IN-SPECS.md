---
id: USER-GUIDES-LIVE-IN-SPECS
kind: issue
title: User-facing guides and their contract references live in the internal specs tree
status: draft
priority: P3
complexity: M
area: [docs, py, web]
design:
created: 2026-11-01
github:
---
## Problem

`guides/PYTHON_GUIDE.md` and `guides/JAVASCRIPT_GUIDE.md` (`audience: user`) and the contract
references they build on (`reference/PYTHON_INTEGRATION.md`, `reference/JAVASCRIPT_INTEGRATION.md`,
`audience: user`) sit in `specs/`, which is internal. Once `docs/` exists
(`design/api-documentation-rules/`), these belong in it ("move, never fork",
`DOCS_STRUCTURE_GUIDE.md` §9.6).

## Impact

The user documentation site starts without its two most complete user guides; users are sent into
the internal tree.

## Expected behaviour

Move each guide with its contract reference into `docs/guides/<language>-developer/`, rewriting
their links into `specs/` (17 at 2026-11-01: 8 in the Python guide, 9 in the JavaScript one) to
`docs/` targets or prose, and updating the paths in `liquers-py/tests/test_GUIDE_examples.py`,
`liquers-web/scripts/test-guide-examples.sh`, `liquers-web/tests/guide/run-guide-examples.mjs`,
seven `liquers-py/src` doc comments, both READMEs, `CLAUDE.md` and
`LANGUAGE-INTEGRATION_GUIDE.md`.

## Discovery

Found in `design/api-documentation-rules/` Phase 2 (2026-11-01), which first planned the move
(Phase 1 question 6) and split it off when the link and path count made it a change of its own.
