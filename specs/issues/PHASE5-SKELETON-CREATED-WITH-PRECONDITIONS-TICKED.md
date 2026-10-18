---
id: PHASE5-SKELETON-CREATED-WITH-PRECONDITIONS-TICKED
kind: issue
title: A new design's Phase 5 skeleton claims every completion precondition is already met
status: draft
priority: P3
complexity: S
area: [docs]
created: 2026-10-18
github:
---

## Problem

`.claude/skills/liquers-project/scripts/init_feature.py` copies the Phase 5 skeleton from
`references/phase5-documentation.md`, whose example section lists its five completion
preconditions as ticked (`- [x] Implementation is finished and validated`, …). Every design created
with the script therefore starts with a Phase 5 document asserting that the implementation is
finished, reviewed and documented, which is false until Phase 5 is written. A reviewer found it in
`specs/design/liquers-py/` on 2026-10-18.

## Expected

The skeleton's preconditions start unticked (`- [ ]`), and the reference keeps the ticked form only
as an example of a finished document, not as the copied template.
