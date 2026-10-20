---
id: VALIDATE-PHASE-EMPTY-SECTION-MATCHES-ACROSS-LINES
kind: issue
title: validate_phase.py reports a filled compact section as empty when its name appears earlier in the phase
status: draft
priority: P3
complexity: S
area: [docs, build]
design:
created: 2026-10-19
github:
---

## Problem

`.claude/skills/liquers-project/scripts/validate_phase.py` `empty_sections` searches with
`(?mis)^{level}#* .*?{name}[^\n]*\n(.*?)(?=^#{{2,{len(level)}}} |\Z)`. Two flaws combine:

- `^###*` (for level `###`) also matches a `##` heading, so the match can start at `## Phase 4`;
- with `s` (DOTALL) and `i`, `.*?` runs across lines and stops at the first case-insensitive
  occurrence of the section name anywhere after that heading, not at the section's own heading.

So in a compact Phase 4 whose `### Steps` text mentions an issue ID containing `VALIDATION` (for
example `close STORE-CONFORMANCE-VALIDATION-TOOL`), group 1 is the empty remainder of that step's
line and the script reports `Validation (empty)` although `### Validation` has content.

## Impact

A false `[FAIL]` on a valid design; the workaround is rewording the design text to avoid the word.
Tooling only, no effect on the product.

## Expected behaviour

Match the section's own heading line: `^{level} [^\n]*{name}[^\n]*\n`, without DOTALL in the
heading part, and require exactly the heading level (or deeper) rather than `##`.

## Discovery

Bulk design of `store-conformance-check-tool`, 2026-10-19: `validate_phase.py … 4` failed with
`Validation (empty)` until step 6 stopped naming its source issue ID.
