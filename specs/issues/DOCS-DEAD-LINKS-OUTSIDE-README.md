---
id: DOCS-DEAD-LINKS-OUTSIDE-README
kind: issue
title: Relative links outside README are never validated
status: closed
priority: P3
complexity: S
area: [docs]
design: docs-current-link-validation
created: 2026-08-08
github:
---
## Problem

`--check` validates link targets only in `specs/README.md` — check 8 in
`DOCS_STRUCTURE_GUIDE.md` §7.2, implemented in `scripts/docs_index.py` (`check()`, the
`re.finditer(r"\]\((?!https?:)…")` loop, which reads `SPECS / "README.md"` and nothing else).
Every other document links unchecked.

A sweep of `specs/**/*.md` resolving relative link targets finds **61 dead links** today, in
three kinds:

1. **Wrong depth into source.** `specs/reference/api/doc-0*.md` link source files as
   `../../liquers-core/src/context.rs`, which from `specs/reference/api/` resolves to
   `specs/liquers-core/…`. Three levels are needed, not two. This accounts for most of the 61.
2. **Absolute paths from a contributor's machine.** `specs/issues/ASSETS-FIX1.md` links
   `/home/orest/zlos/rust/liquers/specs/archive/EXPIRATION-SAFETY.md`.
3. **Targets that moved or never existed.** `specs/design/liquers-web/phase1-high-level-design.md`
   links `../LANGUAGE-INTEGRATION_GUIDE.md` (the guide is at `specs/guides/`);
   `specs/design/dependency-management/DESIGN.md` links `./plan-init-section.md`, absent from the
   folder; `specs/design/liquers-web/phase3-examples.md` links
   `../archive/2026-08-08-issues.md`.

## Impact

Low but steady: a reader following a reference document into the code lands nowhere, and the
rot is invisible because CI is green. It also weakens §8.4's argument — the capability map is
guarded against the decay that killed the old `FEATURES.md`, while the documents it points *at*
are not.

The archive is the one place where dead links are expected and correct: it records what was
true on a date, and `archive/` files are never edited (§2). Any check must skip it.

## Expected behaviour

Check 8 extends to every tracked document: `issues/`, `design/`, `reference/`, `guides/` and
`README.md`, skipping `archive/`. Links into source files are resolved against the repository
root, since that is what a reference document points at. Anchors (`#section`) and absolute URLs
stay out of scope.

Whether the extension lands as an error or a warning is a judgement call: 61 findings arriving
as errors blocks every docs PR until they are fixed, so either the sweep is fixed in the same
change or the new check starts as a warning.

## Discovery

Found while removing `design/liquers-wf/` (2026-08-08). Deleting the folder broke two links in
`design/liquers-web/phase1-high-level-design.md`; `--check` reported only the one in
`README.md`, which is what prompted the sweep.

## Resolution

Closed on 2026-09-01. The documentation check now resolves relative links from every current
README, issue, design, reference, and guide document, while excluding archives, URL targets,
absolute paths, and fragment-only links. The existing current-document corpus was repaired and
Python regressions cover resolution, diagnostics, fragments, URLs, and archive exclusion.
