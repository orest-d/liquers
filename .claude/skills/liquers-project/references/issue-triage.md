# Issue triage and filing

What to do with a new problem, gap or feature: reported by the user, found while designing, or
noticed in passing while fixing something else. Run this **before** creating any file.

`DOCS_STRUCTURE_GUIDE.md` §4.8 remains the only definition of *how* an issue file is created: ID,
template, field vocabularies, index regeneration, and no GitHub issue. This procedure decides
*whether* to create one, and which design it joins.

## Step 0: Describe the problem on an example

Before searching, write down the problem as a concrete example. You will put it into the issue's
**Problem** section, and into Phase 1's `## Problem Example`:

```markdown
**Example.** Evaluating `<query>` against a store holding `<key>` returns `<actual result or error>`.
Expected: `<result>`, because `<the reference or test that says so>`.
```

An input, query, call or scenario; what happens now; what should happen. Validate any Liquers query
in the example with `liquers-validate`. If you cannot construct an example, you do not yet
understand the problem well enough to file it. Say so in the issue body, and file it anyway.

## Step 1: Is it the work in hand?

Apply the overlap tests ([`overlap.md`](overlap.md)) against the issue or design you are currently
working on. If N strongly overlaps it (and no exclusion applies):

**Case 1: extend the current design. Do not file a new issue.**

1. **Phase 1:** add or extend `## Scope Changes`: date, what was added, why (the overlap test that
   held), the problem example, and what it does to acceptance criteria, non-goals and size. Update
   Purpose and acceptance criteria themselves; the section records the change and does not replace
   them. Re-evaluate `complexity` on the source issue and correct it if the scope moved it.
2. **Every later phase that exists:** update it for the new scope. Signatures, risks, examples,
   tests and plan steps must all cover N.
3. **Review each updated phase** as at the end of that phase: the same checklist
   (`review-checklist.md`), the same reviewer roles (sequentially if the host has no parallel
   agents), and `validate_phase.py`. A change in Phase 2 that invalidates Phase 3 or 4 is fixed
   there too, not noted.
4. **Re-label:** readiness, automatic-fix eligibility ([`auto-fix.md`](auto-fix.md)), leading issue
   and open questions in Phase 1's `## Design Readiness`.
5. **Ask the user** (interactive) whether the change needs approval and, if so, to which phase the
   design returns, with a recommendation:
   > The scope grew by <one line>. Does this need your approval? If yes, I suggest returning to
   > **Phase N** (<why: the earliest phase whose content changed materially>). Options: no approval
   > needed (continue where we were) · return to Phase 1 · 2 · 3 · 4.

   Set `phase:` and `status:` in `DESIGN.md` to the chosen return point (`in_review` at that phase).
   If the design was pre-approved (`proceed all`), the scope change voids the pre-approval for every
   phase from the return point on.

   Non-interactive (bulk design, compaction, a routine): do not wait. Record the question in Phase 1
   as **Open design question — scope approval**, recommend the return phase, and set readiness to
   `needs-decision` unless it is already worse.

## Step 2: Does an open design overlap?

Search all open designs and rank strong overlaps ([`overlap.md`](overlap.md), "Choosing the most
suitable candidate"). If one wins and no exclusion applies:

**Case 2: file N and attach it to that design.**

1. File N per `DOCS_STRUCTURE_GUIDE.md` §4.8, with `design: <slug>`, the problem example, and in
   **Discovery** the overlap test(s), the chosen design and the runner-up.
2. Add N's ID to the design's `issues:` (after the leading source). When the design now has more
   than one source, set `merged: <today>` in `DESIGN.md` (§5.1.1). It records that this procedure
   attached the source, and `docs_index.py --check` requires it.
3. Extend the design exactly as in case 1, steps 1-5 (scope change in Phase 1, every existing phase
   updated and reviewed, labels redone, approval question with return phase).

## Step 3: Does an issue without a design overlap?

Search open issues and features that have no `design:`.

**Case 3a: an overlapping issue exists.** File N, and create **one design that solves both** (all
strongly overlapping design-less issues, if there are several). The leading source is the
higher-priority one (on a tie, the older). Set `issues: [<leading>, <others>]`, `merged: <today>`,
and point every source's `design:` at it.

**Case 3b: nothing overlaps.** File N and create a design just for N.

Either way, write the design with the bulk-design rules (`specs/guides/autonomous_bulk_design.md`,
via [`bulk-design.md`](bulk-design.md)):

- produce **all phases, 1 to 4**: carry an open question through as an explicit, recommended
  assumption, and stop at `phase2-blocked` only when no working solution can be stated even under an
  assumption;
- identify and document every required decision in Phase 1's `## Design Readiness`, tiered;
- label **readiness** in `DESIGN.md` and **automatic fixing** in Phase 1;
- record Design Dependencies, including weak overlaps.

Interactive sessions stop here and report. The design waits in the backlog with its decisions listed;
do not start implementing it as a side effect of filing it.

## Labels on the issue

The readiness and automatic-fix labels live on the design. `docs_index.py` projects readiness onto
every source issue's row in `specs/index.csv`, so do not copy it into the issue front-matter. In the
issue body, one line under **Expected behaviour** is enough:
`Designed in design/<slug>/ — readiness: <value>; automatic fixing: <eligible|not eligible>.`
Keep it truthful when the design changes.

## Finish

Regenerate and check the index (`python3 scripts/docs_index.py`, then `--check`), and commit the
issue, design and index together. Then go to [`spin-off.md`](spin-off.md) if you are in the middle of
other work: an eligible item should be fixed now, not left filed.

## Summary

```
new problem N  ──► write the example
   │
   ├─ strong overlap with the work in hand? ──yes──► case 1: extend current design, no new issue
   │
   ├─ strong overlap with an open design? ───yes──► case 2: file N, attach, extend that design
   │
   ├─ strong overlap with design-less issue(s)? ─yes─► case 3a: file N, one design for all
   │
   └─ otherwise ──────────────────────────────────► case 3b: file N, design for N alone
                         every case: review changed phases, label readiness and auto-fix,
                         ask (or record) whether approval is needed and the return phase
```
