---
id: VALIDATE-PHASE-COMPACT-SIZE-IGNORES-SOURCE-COMPLEXITY
kind: issue
title: validate_phase.py checks every compact design against the M size limit
status: draft
priority: P3
complexity: S
area: [docs, build]
design:
created: 2026-10-27
github:
---
## Problem

The `liquers-project` skill limits a compact design to three pages for an `S` source and five for
an `M` source (`SKILL.md` §Design depth). `validate_phase.py` reads the size class from the
design's own front-matter (`fields.get("complexity") or "M"`, in the compact branch of the length
check). But a design has no `complexity` field: `DOCS_STRUCTURE_GUIDE.md` §5 puts complexity on
the source issue. The lookup therefore always falls back to `M`, and no compact design is ever
checked against the `S` limit.

## Problem Example

`specs/design/py-recipe-provider-choice/DESIGN.md` has one `S` source
(`PY-RECIPE-PROVIDER-NOT-CONFIGURABLE`). On 2026-10-27 it was 10,227 characters.
`python3 .claude/skills/liquers-project/scripts/validate_phase.py py-recipe-provider-choice 4`
printed `[OK] Length 10227 characters (limit ~15000)`. It should have warned against the ~9000
`S` limit.

## Impact

Oversized `S` designs pass validation. Reviewers have to measure them by hand, and in the
2026-10-27 Python bulk design several did go over the limit unnoticed.

## Expected behaviour

The compact size check takes its class from the leading source issue (the first entry of
`issues:`, read through the docs index the script already loads), and uses the largest class
among several sources. It falls back to `M` only when no source has a complexity. A test in
`test_validate_phase.py` covers an `S` source over the `S` limit.

## Discovery

Found by an independent reviewer during the 2026-10-27 bulk design of the open Python-related
issues. It was filed without a design because it is a tooling issue outside that run's Python
scope. The next backlog compaction should design it (it is `S`).
