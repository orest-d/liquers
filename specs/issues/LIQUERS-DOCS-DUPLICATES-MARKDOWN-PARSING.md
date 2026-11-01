---
id: LIQUERS-DOCS-DUPLICATES-MARKDOWN-PARSING
kind: issue
title: liquers-docs parses Markdown with its own helpers instead of liquers-markdown
status: draft
priority: P3
complexity: S
area: [docs]
design: 
created: 2026-11-01
github:
---
## Problem

`liquers-docs/src/markdown.rs` has its own line-level Markdown helpers (fenced code, code spans,
ATX headings, section numbers, link targets, front matter), and `ns-docs/include` finds a section
with them. `liquers-markdown` landed on `main` at the same time: a lossless block parser with a
section tree, heading numbers, links and section addresses (`specs/reference/MARKDOWN.md`). The two
were written in parallel and neither knows the other.

## Impact

Two Markdown readers that can disagree (setext headings, indented code, link forms the helpers do
not see). No wrong output is known; the cost is maintenance and possible drift.

## Expected behaviour

`liquers-docs` reads chapters and `specs/` sections through `liquers-markdown` (`Document::parse`,
its `Section` tree and links), and `ns-docs/include` becomes a thin wrapper over a section address,
or is replaced by an `ns-md` command if one returns a section with its subsections.

## Discovery

Merging `main` into the `user-documentation-guides` branch (PR #234), 2026-11-01.
