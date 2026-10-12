---
id: NO-MARKDOWN-NAVIGATION-COMMANDS
kind: feature
title: Markdown documents cannot be navigated, filtered or interpreted by structure
status: draft
priority: P3
complexity: L
area: [lib/commands]
design: 
created: 2026-10-11
github:
---
## Problem

Markdown is the main document format of a Liquers store used as agent memory (`specs/` here, notes
in an agent's memory folder). Liquers has no command that understands its structure: headings and
sections, front-matter, fenced code blocks, links, tables, lists. `store-and-asset-search`
revision 8 adds three narrow pieces:
- `ns-search/md_split` (sections or lines as records, a proof of concept);
- `ns-search/field_extract` (front-matter as columns);
- a title taken from the first heading.

Each was added because search needed it, not as part of a designed tool.

## Impact

An agent can find a document but cannot cheaply ask for "the section headed X", "the outline", "the
links out of this document", "the code blocks in language Y", or "the table under heading Z". So it
reads whole files, spending context it does not need to. The workaround is `md_split`, then a filter,
and only for sections and lines.

## Expected behaviour

A Markdown command set, probably its own namespace, with records as its output so that it composes
with `ns-search` and `ns-rec`. Candidates:
- an outline (heading tree);
- a section by heading path;
- fenced blocks by language;
- links (outgoing; incoming through search);
- tables as records (`liquers-records` already reads Markdown tables);
- front-matter.

`md_split` and `field_extract` should be absorbed or kept as thin aliases. Open questions: one
command with a `unit` or `kind` argument versus several; whether section addressing should be a
query (`-R/doc.md/-/ns-md/section-Phase~.2`); and which Markdown parser to use (`pulldown-cmark` is
already a dependency of the `webui` feature).

## Discovery

Raised by the user on 2026-10-11 while reviewing `store-and-asset-search` revision 8, where `md_split`
was accepted as a proof of concept "looking like a need for a more general markdown navigation,
filtering and interpretation tool" (`specs/design/store-and-asset-search/agent-queries.md` §5).
