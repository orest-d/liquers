---
id: VALIDATE-PHASE-PHASE1-SIZE-COUNTS-EXCLUDED-SECTIONS
kind: issue
title: validate_phase.py counts the problem example and readiness section against Phase 1's size limit
status: draft
priority: P3
complexity: S
area: [docs]
design:
created: 2026-10-31
github:
---

## Problem

The `liquers-project` skill limits Phase 1 to one page "the problem example and readiness section
excluded". `.claude/skills/liquers-project/scripts/validate_phase.py` compares `len(content)` of the
whole file with `MAX_CHARS_FULL[1] = 6000`, so a Phase 1 within the limit is reported
"over the ~6000 size limit: split or trim" whenever its readiness log is long.

## Impact

The warning fires on conforming designs (for example `liquers-py-store` Phase 1: about 12.8 k
characters in all, about 6.4 k without the two excluded sections), so authors learn to ignore it
and a genuinely oversized Phase 1 goes unnoticed. Workaround: measure by hand.

## Expected behaviour

For Phase 1 (and a compact design's `## Phase 1`), subtract the `Problem Example` and
`Design Readiness` sections before comparing with the limit, and say in the message which sections
were excluded.

## Discovery

Writing the `liquers-py-store` design (2026-10-31); confirmed by reading `validate_phase.py`
around the `MAX_CHARS_FULL` comparison.
