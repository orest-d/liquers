---
id: GUIDE-SECTION-ANCHORS-DIFFER-BETWEEN-RENDERERS
kind: issue
title: Section links between chapters break in the EPUB and PDF
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-01
github:
---
## Problem

A link to a section of another chapter (`05-commands.md#52-arguments-and-defaults`) uses the
heading identifier mdBook generates. In the single-file Pandoc input for EPUB and PDF,
`liquers-docs` rewrites it to `#52-arguments-and-defaults` (`liquers-docs/src/build.rs`
`pandoc_links`), but Pandoc derives identifiers differently (it drops leading digits and
punctuation, giving `arguments-and-defaults`). Links to a whole chapter work: the chapter's first
heading gets an explicit identifier.

## Impact

Section links between chapters are dead in the EPUB and PDF. None exist yet (all chapters are
outlines).

## Expected behaviour

Section links resolve in every format: either `liquers-docs` gives each heading in the Pandoc input
the identifier mdBook would generate, or it maps the fragment to Pandoc's.

## Discovery

`design/user-documentation-guides` Phase 4, Step 2.
