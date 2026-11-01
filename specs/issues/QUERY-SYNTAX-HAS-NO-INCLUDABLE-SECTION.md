---
id: QUERY-SYNTAX-HAS-NO-INCLUDABLE-SECTION
kind: issue
title: Query Language section of PROJECT_OVERVIEW cannot be included in the user guides
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-01
github:
---
## Problem

Appendix A of the user guides (`docs/guides/*/a-query-syntax.md`) should include the query syntax
from `specs/` rather than copy it, so the two cannot drift. The obvious source,
`reference/PROJECT_OVERVIEW.md` § *1. Query Language*, holds relative links (to `REALMS.md`,
`RECIPES_AND_PLANS.md` and an issue), and `ns-docs/include` refuses a section with a relative link
because the link would break inside a chapter (`liquers-docs/src/include.rs`). No other `specs/`
document has a link-free syntax table.

## Impact

Appendix A stays an outline. Whoever writes it either copies the syntax (and it drifts) or waits
for an includable section. No user is affected until the guides are published with appendix A
`ready`.

## Expected behaviour

A link-free subsection — a syntax table of segments, headers, actions, arguments and escapes — in
`PROJECT_OVERVIEW.md` or `QUERY_ESCAPING_GUIDE.md`, included by appendix A with
`{{$ -R/specs/…/-/ns-docs/include-…}}`.

## Discovery

Writing the guide skeletons in `design/user-documentation-guides` Phase 4, Step 3.
