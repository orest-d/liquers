---
id: DOCS-LINK-CHECK-READS-CODE-SPANS
kind: issue
title: The docs dead-link check treats link syntax inside code spans and code blocks as links
status: closed
priority: P3
complexity: S
area: [docs, build]
design: docs-link-check-code-spans
created: 2026-10-04
github:
---

## Problem

`scripts/docs_index.py` finds relative links with `RELATIVE_LINK_RE`
(`\]\((?!https?://)([^)]+)\)`) over the raw text of every tracked Markdown file
(`relative_link_errors`). It does not skip inline code spans or fenced code blocks, so a document
that *quotes* link syntax — a Python f-string building a link, an example of a broken link, a
before/after table of generated output — fails `--check` with "dead link".

Seen on 2026-10-04 writing `design/docs-index-phase-link-targets/`: a quoted f-string
building a link from `x.relative_to(SPECS).as_posix()` inside a ```` ```python ```` block was reported
as a dead link to `{x.relative_to(SPECS`, and example link targets in backticks were reported too.
`DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS` had already worked around the same thing by splitting the
expression across lines ("shown split so this file's own link check does not read it as a link").

## Impact

Authors of design and issue documents about the docs tooling — or any document quoting Markdown —
must contort examples to pass the check. Low severity: the workaround is always possible, and the
check errs on the side of reporting.

## Expected behaviour

Link syntax inside inline code spans and fenced code blocks is not validated, matching how
Markdown renders it (as text, not a link).

## Discovery

`design/docs-index-phase-link-targets/` Phase 2-3 drafting, 2026-10-04.

## Resolution (2026-10-06)

Fixed by `design/docs-link-check-code-spans/`. `scripts/docs_index.py` gained `blank_code`, which
replaces fenced blocks (```` ``` ```` or `~~~`, closed by a fence of the same character and at
least the same length) and inline code spans (a backtick run closed by the next run of the same
length; an unclosed run stays literal) with spaces, keeping newlines and offsets.
`relative_link_errors` now matches `RELATIVE_LINK_RE` against the blanked text. Indented code
blocks are not recognized, as documented in the docstring. The only other raw-text regex over
Markdown (`specs/README.md` issue IDs) reads code spans on purpose and is unchanged.

Evidence: `BlankCodeTests` in `scripts/test_docs_index.py` (fenced, tilde fence, nested fence
lengths, inline span, double-backtick span with an inner backtick, unclosed backtick, length and
newline preservation, end-to-end through `relative_link_errors`); `python3 scripts/docs_index.py
--check` reports 0 errors on the tree, as before. `DOCS_STRUCTURE_GUIDE.md` §7.2 check 9 states
the rule.
