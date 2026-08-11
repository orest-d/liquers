# Phase 5: Documentation

Phase 5 records what was actually implemented and makes the result understandable without requiring a reader
to inspect the design history. It is mandatory for every `workflow: liquers-project` design.

## Entry Gate

Start only after:

- implementation is finished and validated;
- all user comments are answered or incorporated;
- all review comments are answered or incorporated; and
- the documentation can be checked against the implemented and tested behavior.

Complete Phase 5 in the implementation PR before merge when practical. Merge is not an entry
condition. If a rebase, merge conflict, or integration change later makes the documentation
inconsistent, review the affected material after merge and fix it.

Set `phase: documentation` in `DESIGN.md`. When `gh_pr` is present, do not add the derived
`in_implementation` or `implemented` status. Do not mark the design `complete` yet.

## Inputs

Read all Phase 1-4 documents, the implemented behavior and tests, user and review feedback, commit
or PR outcomes, and issues filed during the effort. Use the documentation intent from Phase 1 and
the exact documentation architecture from Phase 2 as the baseline, then revise that plan if the
implementation produced important new information.

## Required Outputs

### Phase 5 summary

Create `specs/design/<slug>/phase5-documentation.md` from the template below. Target about one page
and never exceed three pages. Link to a reference or guide rather than expanding the summary beyond
that limit.

The summary must state:

- what was implemented and tested;
- whether it conforms to the original request and approved design;
- what was omitted or added, and why;
- what new issues were filed;
- important learning points, if any; and
- where current reference and task guidance now live.

A short summary is required even when no reference or guide is created.

### Reference and guide documents

Create the documents requested in Phases 1-2:

- Put descriptions of current behavior, intent, meaning, importance, architecture, and connections in
  `specs/reference/`.
- Put repeatable instructions for using, extending, testing, debugging, or doing similar work in
  `specs/guides/`.

Use `SCREAMING_SNAKE_CASE.md` names and the front matter from `DOCS_STRUCTURE_GUIDE.md` §9.1. Every
new or substantively updated document ends with `## History`; the newest row uses the same date as
`reviewed:` and `phase-5` as its source.

If Phases 1-2 selected neither a reference nor a guide, reconsider that decision when the accumulated
information is too substantial for the summary or would materially help coding agents and developers.
Record the changed decision in the Phase 5 summary.

### Existing documentation

Generate candidates from every `specs/reference/` and `specs/guides/` document sharing an `area`
with the design. Keep or discard each candidate deliberately and write the kept paths to
`affects_docs` in `DESIGN.md`. `affects_docs` is authoritative.

Review every kept document against implemented code and tests, not the design. Update inaccurate claims or
instructions. Even when no prose change is needed, add a History row recording the review and bump
`reviewed:` to the same date.

### Links and capability map

Update `specs/README.md` in the same change. Once a capability has current reference or guide
documentation, its capability-map entry should link to that highest-stage document instead of the
design folder. Add or update other cross-links needed to anchor the new capability in existing
documentation. Do not make readers inspect a design to understand current behavior or normal use.

### Issues

File an issue for intentionally omitted design scope, newly discovered defects, gaps, limitations,
or follow-up work. A design is never partially complete: shipped work belongs to the completed
design and unfinished work belongs to linked issues.

### Status maintenance

For every issue or feature this work completes, update its front-matter to `status: closed` during
Phase 5 and add a concise resolution note with its evidence (for example, tests, a commit, or a
PR). Use `closed_not_planned` when work is deliberately not pursued. This local record is
authoritative even when `github:` is present; GitHub metadata must not overwrite it.

## Summary Template

```markdown
# Phase 5: Documentation - <Feature Name>

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical

## Implementation Summary

<About one page; never more than three pages for this whole document. State what was implemented
and whether it conforms to the request and approved design. Identify anything omitted or added and
explain why. Link to reference/guide documents for detail.>

## Documentation Delivered

### New Reference Documents
<Paths and purposes, or `None` with rationale>

### New Guide Documents
<Paths and purposes, or `None` with rationale>

### Existing Documents Reviewed or Updated
<Authoritative `affects_docs` set, review result, and History/reviewed updates>

### Links and Capability Map
<Links added, updated, or replaced in `specs/README.md` and other documentation>

## Issues Filed

<New issue IDs and one-line explanations, including intentionally omitted design scope; or `None`>

## Important Learning

<Meaning and importance of the work, connections to existing functionality, repeatable guidance,
corrections, and unexpected learning. Keep details in reference/guide documents and link them here.>

## Conformance and Remaining Work

<Compare requested, approved, and implemented scope explicitly. State whether anything remains. Every
deferred remainder must have an issue rather than a partial design status.>

## Validation

<Documentation checks run and outcomes.>
```

## Review Checklist

- [ ] Entry-gate conditions are evidenced, not assumed
- [ ] Summary is present, roughly one page, and no more than three pages
- [ ] Requested, approved, and implemented scope are compared explicitly
- [ ] Every omission or addition has a reason
- [ ] New issues are listed, including all deferred work
- [ ] Every completed issue or feature has its authoritative local status and resolution note updated
- [ ] Important learning, corrections, and connections are captured
- [ ] Planned reference/guide documents exist, or a justified decision records why not
- [ ] Reference documents state current behavior and meaning without relying on the design
- [ ] Guides provide actionable, repeatable instructions without relying on the design
- [ ] `affects_docs` is authoritative and every listed document was reviewed against implemented code
- [ ] Every reviewed document has matching `reviewed:` and newest History dates
- [ ] `specs/README.md` and other relevant links point to the highest-stage current document
- [ ] Documentation checks pass
- [ ] If integration changed relevant content, post-merge consistency was rechecked

## Completion Gate

Present the Phase 5 summary and all documentation changes to the user. Stop and wait for the exact
approval keyword `proceed`. After approval, set `status: complete`, remove `phase`, update the Phase
5 checkbox in `DESIGN.md`, and run the documentation checks again. The folder is frozen once the
design reaches `complete`.
