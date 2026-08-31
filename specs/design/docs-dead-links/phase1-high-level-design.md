# Phase 1: High-level design

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** `docs_index.py` already owns validation and has a narrowly scoped check-eight loop; tracked document roots and archive exclusion are explicit.
- **Open questions:** None.

## Problem and outcome

`docs_index.py --check` validates relative links only in `specs/README.md`; dead links in issues, designs, references, and guides remain invisible. Extend check eight to scan every tracked Markdown document except `archive/`, resolve relative targets from each document, and make findings errors after correcting the known corpus.

Acceptance criteria: valid sibling, parent, and repository-source links pass; anchors and absolute URLs are ignored; archive documents are ignored; a temporary dead relative link yields a path-specific diagnostic; all current non-archive dead links are fixed in the same change.

## Scope and constraints

Affected files are `scripts/docs_index.py`, its tests, and non-archive Markdown links found by the sweep. Links must be resolved relative to the document that contains them; repository-root source links therefore need their actual relative depth. Do not mutate archive history, validate network URLs, or turn this into a generic Markdown parser.

## Design Dependencies

- None.

## Documentation assessment

Update `specs/DOCS_STRUCTURE_GUIDE.md` check-eight wording and history because it is the validator contract; the generated README/index follow normal regeneration.

## Consolidated Findings

The existing regular expression intentionally has a limited target grammar. Keeping that scope, stripping an optional fragment before filesystem lookup, and using each file's parent directory fixes the documented failures without checking remote URLs. Correcting the known findings in the same commit permits an error-level invariant without blocking all documentation changes.
