# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The expected behaviour is Markdown's own: link syntax inside code is text. The
  implementation is a pre-pass that blanks code before applying `RELATIVE_LINK_RE`.
- **Open questions:** None

## Problem

`scripts/docs_index.py` runs `RELATIVE_LINK_RE` (`\]\((?!https?://)([^)]+)\)`) over the raw text
of every tracked Markdown file (`relative_link_errors`). Link syntax quoted in inline code or in a
fenced block is reported as a dead link, which forces authors to contort examples (seen in
`design/docs-index-phase-link-targets/` and `DOCS-INDEX-EMITS-MACHINE-LOCAL-PATHS`).

## Expected behaviour and acceptance

1. A link inside a fenced block (```` ``` ```` or `~~~`, any info string) is not checked.
2. A link inside an inline code span (backtick runs of any length, matched by equal length) is not
   checked.
3. A real link outside code is still checked. A dead one still fails `--check`.
4. Reported line numbers (if the checker reports them) are unchanged. Blanking preserves
   newlines.
5. `python3 scripts/docs_index.py --check` passes on the current tree with the same result as
   before (no previously-valid link becomes unchecked by accident, apart from links in code).

## Scope

`relative_link_errors` in `scripts/docs_index.py`. Any other regex pass over raw text (e.g. phase
link targets) is checked for the same need and fixed consistently.

## Design Dependencies

- `docs-index-phase-link-targets` — **overlaps** (same script; that design found this issue).

## Documentation assessment

- `specs/DOCS_STRUCTURE_GUIDE.md`, the section that describes the dead-link check (search "dead
  link"): one sentence "links inside code are not checked". It is a guide-like contract document,
  so it gets a History row if it has one.

## Consolidated Findings

- A CommonMark parser dependency is unnecessary. A line-based fence tracker plus an inline
  backtick-run scanner covers the cases the repository uses. Indented code blocks (4 spaces) are
  rare in specs and conflict with list continuation, so do not treat them as code, and say so in
  the function's docstring.
- Replace code characters with spaces (keeping `\n`), so offsets and line numbers stay valid.
