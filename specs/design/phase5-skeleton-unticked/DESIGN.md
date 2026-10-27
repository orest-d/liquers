---
id: PHASE5-SKELETON-UNTICKED
kind: design
title: A new design's Phase 5 skeleton starts with its completion preconditions unticked
form: compact
status: complete
gh_pr: [188]
readiness: ready
autofix: eligible
area: [docs]
issues: [PHASE5-SKELETON-CREATED-WITH-PRECONDITIONS-TICKED]
created: 2026-10-20
---
# A new design's Phase 5 skeleton starts with its completion preconditions unticked

> **Complete on 2026-10-27**: implemented by automatic fix in #188.

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written; Phase 4
implemented on 2026-10-27 (automatic fix).

## Phase 1: High-Level Design

### Purpose

A freshly created Phase 5 document must not claim that the implementation is finished, reviewed
and documented. Its entry-gate checklist starts unticked and is ticked with evidence.

### Problem Example

At HEAD (18732dd), `python3 init_feature.py demo` (full form, with workflow) writes
`specs/design/demo/phase5-documentation.md` from the first fenced `markdown` block of
`.claude/skills/liquers-project/references/phase5-documentation.md`, whose
`## Completion Preconditions` (`:100-106`) lists all five items as `- [x]`. The new file says
"`- [x] Implementation is finished and validated`" before Phase 1 is written. Two open designs
carry exactly that untouched skeleton today: `agent-memory-derived` and `external-index-sync`
(both `status: draft`).

Should be: the five items are `- [ ]` in the created file.

### Scope and Acceptance Criteria

- **AC-1** Unticked skeleton
  WHEN `init_feature.py <slug>` creates a full-form design with the workflow
  THEN no created file contains a ticked box `- [x]`
- **AC-2** The reference still shows the finished form
  WHEN a reader opens `references/phase5-documentation.md`
  THEN the template shows `- [ ]` and the text says each item is ticked when its evidence exists

Non-goals: editing completed designs' Phase 5 documents (frozen, and truthful); a
`validate_phase.py` check of Phase 5 ticks.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** eligible — documentation/skill fix in one reference file, a test for the
  skill script, and unticking two untouched generated skeletons; no interface change.
- **Leading issue:** None
- **Explanation:** The template is the only source (`init_feature.py` `skeleton()` copies it), so
  changing five characters in it fixes every future design.
- **Open questions:** None.

### Design Dependencies

- `validate-phase-section-match` — overlaps (same skill's scripts and first test file); can share a
  PR, independent order.
- `DOCS-TOOLING-TESTS-NOT-RUN-IN-CI` — overlaps: the new test runs locally only until fixed.

### Consolidated Findings

The fix is in the reference template, not in `init_feature.py`. The two draft designs that already
carry the untouched skeleton are corrected in the same change; completed designs are never edited.

## Phase 2: Architecture

### Solution

1. `references/phase5-documentation.md`, Summary Template block: the five `- [x]` become `- [ ]`;
   add one sentence above the block: "Tick each precondition when its evidence exists, and name the
   evidence (commit, PR, test run) on the line."
2. `specs/design/agent-memory-derived/phase5-documentation.md` and
   `specs/design/external-index-sync/phase5-documentation.md`: untick, only if each file is still
   byte-identical to the old skeleton apart from the title (checked with `diff` against the
   template block); otherwise leave it to that design's owner.

Rejected: post-processing in `init_feature.py` (`[x]`→`[ ]`), which would leave the template, the
documented single source, wrong.

### Changes

The two documents above plus a new test file
`.claude/skills/liquers-project/scripts/test_init_feature.py` (stdlib `unittest`, loads the script
by path, runs `init_feature()` with the working directory set to a temporary tree containing
`specs/`).

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A design's Phase 5 author forgets to tick | The review checklist already asks "Entry-gate conditions are evidenced, not assumed" | high |
| Editing a design that is not ours | Only untouched generated skeletons of draft designs, verified by diff | high |

## Phase 3: Examples and Tests

### Examples

`python3 init_feature.py demo` in a scratch tree; `grep -c '\- \[x\]' specs/design/demo/*.md`
prints `0` for every file.

### Tests

`test_init_feature.py`, class `SkeletonTests`:

- `test_full_form_has_no_ticked_box` — creates `demo` (full, workflow) in a temporary tree; no
  created file contains `- [x]` — AC-1
- `test_phase5_template_preconditions_unticked` — the `## Completion Preconditions` list of
  `skeleton("phase5-documentation.md")` has five `- [ ]` items and the reference text contains the
  word "evidence" above the block — AC-2

Run: `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'`.

## Phase 4: Implementation Plan

### Steps

- [x] 1. `test_init_feature.py` with both tests; they fail at HEAD — the unittest command above
- [x] 2. `references/phase5-documentation.md` — untick the five items, add the evidence sentence — tests pass
- [x] 3. Untick the two untouched draft skeletons after the `diff` check — `grep -L '\- \[x\]' specs/design/{agent-memory-derived,external-index-sync}/phase5-documentation.md` lists both
- [x] 4. Close the source issue with a resolution note — `python3 scripts/docs_index.py --check`

### Validation

The unittest command and `docs_index.py --check`. Diff review: one reference file, one test file,
at most two generated skeletons.
