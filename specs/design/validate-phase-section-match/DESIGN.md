---
id: VALIDATE-PHASE-SECTION-MATCH
kind: design
title: validate_phase.py finds a compact section's body from its own heading line
form: compact
status: in_review
phase: implementation
readiness: ready
autofix: eligible
area: [docs, build]
issues: [VALIDATE-PHASE-EMPTY-SECTION-MATCHES-ACROSS-LINES]
created: 2026-10-20
---
# validate_phase.py finds a compact section's body from its own heading line

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

`validate_phase.py` must not report a filled compact section as empty because the section's name
occurs in an earlier section's text.

### Problem Example

Reproduced at HEAD (18732dd) by calling `empty_sections` from
`.claude/skills/liquers-project/scripts/validate_phase.py` on the body of a compact Phase 4
section (the `## Phase 4` heading omitted here, for the reason in AC-4):

```markdown
### Steps

- [x] 1. Do x — close STORE-CONFORMANCE-VALIDATION-TOOL — `cargo test`

### Validation

Run the tests.
```

`empty_sections(content, REQUIRED_COMPACT[4], "###")` returns `['Validation']`. The pattern
`(?mis)^{level}#* .*?{name}[^\n]*\n(.*?)(?=…)` starts at `### Steps`; with DOTALL and IGNORECASE,
`.*?` runs over the line break and stops at `VALIDATION` inside the step, so the captured body is
the empty rest of that line. Should return `[]`.

A second defect of the same kind surfaced while validating this design: `compact_section` (and
`scripts/docs_index.py` `phase_text`, `:899`) split phases at the first `^## Phase N` line *even
inside a fenced code block*. With the example above written in full in Phase 1,
`validate_phase.py … 1` reported Phase 1's later sections missing and `… 4` validated the fenced
example instead of the real Phase 4.

The issue's first suspected flaw does not hold: with `level = "###"` the pattern begins `####*`,
three `#` and then any number, so it never matches a `##` heading. Only the cross-line `.*?` is
wrong.

### Scope and Acceptance Criteria

- **AC-1** Name in earlier text
  WHEN a required compact section has content and its name appears, in any case, in the text of an
  earlier section
  THEN `empty_sections` does not report it
- **AC-2** Really empty
  WHEN a required section's heading is followed directly by the next `##`/`###` heading or the end
  THEN `empty_sections` reports it
- **AC-3** Deeper headings are content
  WHEN a `###` section contains only a `####` subsection with text
  THEN it is not reported empty (unchanged behaviour)

- **AC-4** Fenced phase headings
  WHEN a compact `DESIGN.md` contains a fenced code block with a `## Phase N` line
  THEN `validate_phase.py` and `docs_index.py --check` take each phase from its real heading, not
  from the fenced line

Non-goals: `missing_sections` (its pattern has no DOTALL and is correct); the full-form checks.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — two regex fixes in tooling scripts (`validate_phase.py`,
  `docs_index.py`) plus stdlib tests; no interface change.
- **Leading issue:** None
- **Explanation:** The intended behaviour is stated in the function's docstring; the fix confines
  the heading match to one line.
- **Open questions:** None.

### Design Dependencies

- `phase5-skeleton-unticked`, `front-matter-yaml-agreement` — overlaps (same tooling, separate
  change sites); can share one PR.
- `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI` (issue filed with this design) — overlaps: the new test runs
  locally only until that is fixed.

### Consolidated Findings

Two pattern fixes: the heading match confined to one line, and phase splitting that ignores fenced
code (in both scripts, via `docs_index.blank_code`'s technique). The issue's first suspected flaw
does not hold and its body is corrected; AC-4 is added to it. There are no tests for the skill
scripts today, so the plan adds the first one beside the script.

## Phase 2: Architecture

### Solution

In `empty_sections`, replace the heading part `.*?{name}[^\n]*\n` (under DOTALL) with a
single-line match, and keep DOTALL only for the body:

```python
rf"(?mi)^{level}#* [^\n]*?{re.escape(name)}[^\n]*(?:\n|\Z)((?s:.*?))(?=^#{{2,{len(level)}}} |\Z)"
```

`(?:\n|\Z)` also handles a heading on the last line. Rejected: dropping `s` and using `[\s\S]*?`
for the body (equivalent, less readable).

For AC-4, `compact_section` searches for the `## Phase N` boundaries in a copy of the text whose
fenced blocks are blanked to the same length (the existing `FENCED_CODE` pattern, replacing every
non-newline character with a space), then slices the original text at those offsets. `docs_index.py`
`phase_text` does the same with its own `blank_code` (`:225`), which already preserves offsets.

### Changes

- `.claude/skills/liquers-project/scripts/validate_phase.py` `empty_sections`: the pattern above.
- Same script, `compact_section`: boundaries found on fence-blanked text (AC-4).
- `scripts/docs_index.py` `phase_text`: boundaries found on `blank_code(text)`; new test
  `PhaseTextTests.test_fenced_phase_heading_is_ignored` in `scripts/test_docs_index.py`.
- New `.claude/skills/liquers-project/scripts/test_validate_phase.py`: stdlib `unittest`, loads the
  script with `importlib.util.spec_from_file_location` (the directory is not a package).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A design that passed only thanks to the bug now fails | It cannot: the bug only adds false `empty` reports | high |
| A fence-blanking bug shifts offsets | Blank to equal length; AC-4 tests compare the sliced text | high |
| A heading naming two sections (`### Examples and Tests`) | First match per name, as today | high |

Recovery: revert the script.

## Phase 3: Examples and Tests

### Examples

The Problem Example returns `[]`; deleting `Run the tests.` makes it return `['Validation']`.
`python3 validate_phase.py store-conformance-check-tool 4` passes whether or not its step names
the source issue ID.

### Tests

In `test_validate_phase.py`, class `EmptySectionsTests`:

- `test_name_in_earlier_section_text_is_not_empty` — the Problem Example — AC-1
- `test_heading_followed_by_heading_is_empty` and `test_heading_at_end_is_empty` — AC-2
- `test_deeper_subsection_counts_as_content` — `### Examples` holding `#### Primary` + text — AC-3
- `CompactSectionTests.test_fenced_phase_heading_is_ignored` — a Phase 1 holding a fenced
  `## Phase 4` block; `compact_section(text, 4)` returns the real Phase 4 — AC-4
- `scripts/test_docs_index.py` `PhaseTextTests.test_fenced_phase_heading_is_ignored` — the same
  through `phase_text` — AC-4

Run: `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'` and
`python3 -m unittest scripts/test_docs_index.py`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `test_validate_phase.py` with the four tests; AC-1 fails at HEAD — `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'`
- [x] 2. `validate_phase.py` `empty_sections` — the Phase 2 pattern — same command, all pass
- [x] 3. `compact_section` and `docs_index.py` `phase_text` on fence-blanked text — both unittest commands pass (AC-4)
- [x] 4. Re-run `validate_phase.py <slug> 1..4` on every compact design with `readiness` (loop over `specs/design/*/DESIGN.md` with `form: compact`) — no new `[FAIL]`
- [x] 5. Close the source issue with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

Step 4 is the regression sweep, together with `python3 scripts/docs_index.py --check` (whose
scenario checks use `phase_text`). Diff review: two scripts, one new test file, one new test class.

### Implementation notes (2026-10-27)

- The `docs_index.py` function is `design_phase_text`, not `phase_text`; the new test is a new
  class `PhaseTextTests`.
- The step 4 sweep turned up one design whose fences were unbalanced
  (`dependency-summary-reuse`, two stray fence lines left by a review edit). Read with fence-aware
  phase splitting, its Phase 3 and 4 disappeared into the open fence. The stray lines are removed;
  the sweep then shows only fixed false reports (`command-registry-header-feature-names` and
  `search-read-dependencies` Phase 2), no new `[FAIL]`.
