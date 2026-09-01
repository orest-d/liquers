# Phase 5: Documentation - Tracked specification links

## Implementation Summary

`docs_index.py --check` now validates relative filesystem targets in every current README, issue,
design, reference, and guide document. Resolution starts at the containing document; fragments are
removed before lookup; HTTP(S), absolute-path, and fragment-only targets are ignored; archive
documents are not scanned. The existing current-document link corpus was repaired.

## Documentation Delivered

`specs/DOCS_STRUCTURE_GUIDE.md` now records the expanded check-nine contract and its exclusions.
The affected language-integration and unit-test guides carry corrected links, updated review
dates, and History entries. No new reference or user guide was necessary.

## Validation and Remaining Work

Python unit tests cover sibling/parent and repository-source resolution, missing-target
diagnostics, fragments, URLs, and archive exclusion. The full documentation check passes with only
pre-existing classification and staleness warnings. Network URL validation and generic Markdown
parsing remain explicit non-goals; no scoped work or documentation proposal remains.
