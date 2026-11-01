---
id: DRAFT-MARKERS-LOOK-ALIKE
kind: issue
title: Draft builds show every editorial marker the same way
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-01
github:
---
## Problem

In a draft build, `liquers-docs` turns every editorial marker into a quoted paragraph
(`> **TBD:** …`, `liquers-docs/src/markers.rs` `apply_markers`). `guide-outlines.md` §4 asks for
`TBD:` and `CHECK:` to be highlighted and `PURPOSE:`, `SOURCE:`, `EDITOR:` to be muted. Plain
CommonMark has no way to give a blockquote a class, so `docs/theme/liquers.css` styles them all
alike.

## Impact

Editors reading a draft cannot tell open work from writer notes at a glance. Cosmetic; release
builds are unaffected.

## Expected behaviour

Each marker kind is visually distinct in draft HTML: for example the build tree (not the chapter
source, which stays tool-neutral) emits a raw-HTML wrapper with a `marker-tbd` … class, or a
distinct leading symbol per kind.

## Discovery

`design/user-documentation-guides` Phase 4, Step 8 (local build).
