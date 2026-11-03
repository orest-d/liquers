---
id: USER-GUIDES-LIVE-IN-SPECS
kind: feature
title: Liquers has no public user guides, in any format, and nothing publishes documentation on the web
status: closed
priority: P2
complexity: L
area: [docs, build]
design: user-documentation-guides
created: 2026-11-01
github:
---
## Problem

All of Liquers' how-to material lives in `specs/guides/`. These are short, topic-specific guides
that agents maintain alongside the code: they address contributors and coding agents, are updated
by every design that touches them, and move with the code. The user-audience ones among them
(`PYTHON_GUIDE.md`, `JAVASCRIPT_GUIDE.md`, `ENVIRONMENT_CONSTRUCTION_GUIDE.md`, …) are the nearest
thing to user documentation there is.

What does not exist is documentation **written for users**: larger, book-like documents that
introduce Liquers, teach it in order and are edited by hand for quality. Nothing is published
either: there is no website, no GitHub Pages deployment and no CI job that builds or deploys any
documentation.

## Impact

A newcomer must read internal, agent-oriented guides that assume the codebase, in no particular
order. Nothing can be linked from a README, a package index or an announcement.

## Expected behaviour

The owner on 2026-11-01:

1. `specs/guides/` stays: agent-maintained, topic-specific guides. Nothing moves out of it; it is
   the basis from which the public guides are written.
2. Public user guides in `docs/guides/` are larger, book-like documents, possibly hand-edited,
   held to a high quality standard, covering the `specs/guides/` topics as chapters. Three to
   start: Liquers for Rust, for Python and for the Web, with parallel structure.
3. They can be generated as HTML (with and without embedded search), EPUB and possibly PDF (via
   LaTeX or Typst), are visible on the web through GitHub Pages, and `main` holds only sources.
4. A skeleton, a build script and the CI that builds and publishes them are part of the same
   design, as are an editing guide and a guide to the GitHub Actions.

## Discovery

First filed on 2026-11-01 by `design/api-documentation-rules/` Phase 2 as "move the user-audience
guides from `specs/` to `docs/`". The same day the owner rejected moving them and restated the
need as above, refined in a second review (guides, not books; the formats). The ID is kept, as IDs are never renamed (`DOCS_STRUCTURE_GUIDE.md` §4.1), and the
title and body were rewritten.

## Resolution (2026-11-01)

Implemented by `design/user-documentation-guides` (PR #234). `docs/guides/{rust,python,web}/` holds
three parallel guides (20 outline chapters each, written from `specs/` through their `sources:`),
`liquers-docs` expands and checks them, `scripts/build-docs.sh` renders HTML with and without
search, EPUB and PDF, and `.github/workflows/docs-site.yml` publishes the site to GitHub Pages
from `main`. How to write chapters: `specs/guides/USER_DOCS_EDITING_GUIDE.md`; CI:
`specs/guides/GITHUB_ACTIONS_GUIDE.md`. Writing the chapters is editorial work, not part of this
feature; follow-ups are filed separately (`USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED`,
`QUERY-SYNTAX-HAS-NO-INCLUDABLE-SECTION`).
