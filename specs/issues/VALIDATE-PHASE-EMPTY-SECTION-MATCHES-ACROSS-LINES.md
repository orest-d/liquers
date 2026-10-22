---
id: VALIDATE-PHASE-EMPTY-SECTION-MATCHES-ACROSS-LINES
kind: issue
title: validate_phase.py reports a filled compact section as empty when its name appears earlier in the phase
status: draft
priority: P3
complexity: S
area: [docs, build]
design: validate-phase-section-match
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

**Correction (bulk design, 2026-10-20).** The first bullet does not hold: with `level = "###"`
the pattern begins `####*` — three `#`, then any number — so it never matches a `##` heading. The
second bullet is the whole cause, reproduced at main 18732dd by calling `empty_sections` directly.

A related defect of the same kind, found while validating the design: `compact_section` here and
`scripts/docs_index.py` `phase_text` split a compact `DESIGN.md` at the first `^## Phase N` line
even when it is inside a fenced code block, so a design that quotes a phase heading in an example
is validated against the wrong text. The design covers both.

## Problem Example

A compact Phase 4 whose `### Steps` names `STORE-CONFORMANCE-VALIDATION-TOOL` and whose
`### Validation` reads `Run the tests.`: `empty_sections(…, REQUIRED_COMPACT[4], "###")` returns
`['Validation']` today and should return `[]`.

## Impact

A false `[FAIL]` on a valid design; the workaround is rewording the design text to avoid the word.
Tooling only, no effect on the product.

## Expected behaviour

Match the section's own heading line: `^{level} [^\n]*{name}[^\n]*\n`, without DOTALL in the
heading part, and require exactly the heading level (or deeper) rather than `##`.

## Discovery

Bulk design of `store-conformance-check-tool`, 2026-10-19: `validate_phase.py … 4` failed with
`Validation (empty)` until step 6 stopped naming its source issue ID.
