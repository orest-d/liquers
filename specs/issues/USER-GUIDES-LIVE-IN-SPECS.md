---
id: USER-GUIDES-LIVE-IN-SPECS
kind: feature
title: Liquers has no public, book-length user documentation, and nothing publishes documentation on the web
status: accepted
priority: P2
complexity: L
area: [docs, build]
design: user-documentation-books
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

1. `specs/guides/` stays: agent-maintained, topic-specific guides. Nothing moves out of it.
2. Public user documentation consists of larger, book-like documents, possibly hand-edited, held
   to a high quality standard.
3. The documentation is visible on the web, for example through GitHub Pages.
4. The CI that builds and publishes it is part of the same design.

## Discovery

First filed on 2026-11-01 by `design/api-documentation-rules/` Phase 2 as "move the user-audience
guides from `specs/` to `docs/`". The same day the owner rejected moving them and restated the
need as above. The ID is kept, as IDs are never renamed (`DOCS_STRUCTURE_GUIDE.md` §4.1), and the
title and body were rewritten.
