---
id: LIQUERS-DOCS-HAS-NO-LIVE-PREVIEW
kind: feature
title: liquers-docs is not an mdBook preprocessor, so there is no live preview
status: draft
priority: P3
complexity: M
area: [docs]
design: 
created: 2026-11-01
github:
---
## Problem

`liquers-docs build` writes a build tree that mdBook then renders (`tooling.md` §6, step 1). An
editor sees a change only after rerunning both; `mdbook serve`'s live reload shows the build tree,
not the chapter sources, because the template expansion happens outside mdBook.

## Impact

Slower editing of the guides. No user impact.

## Expected behaviour

`liquers-docs` also speaks mdBook's preprocessor protocol (step 2 of the path in
`design/user-documentation-guides/tooling.md` §6), so `mdbook serve` over `docs/guides/<guide>/`
expands templates on every change. This is the first step toward the Liquers-based documentation
tool the owner wants (steps 3-4: an mdBook backend writing search records for `ns-search`, then
Liquers rendering the site itself).

## Discovery

`design/user-documentation-guides` Phase 1 (tool evaluation); filed at Phase 5 as the next step.
