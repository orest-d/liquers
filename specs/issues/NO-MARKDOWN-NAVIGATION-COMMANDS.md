---
id: NO-MARKDOWN-NAVIGATION-COMMANDS
kind: feature
title: Markdown documents cannot be navigated, filtered or interpreted by structure
status: closed
priority: P3
complexity: L
area: [lib/commands]
design: markdown-navigation
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

## Problem Example

An agent wants the outline of `specs/reference/AGENT_MEMORY_SERVICE.md`, the `yaml` code blocks
under its §3 and the table in its `## History` section as records. The nearest queries are

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-search/md_split-section
-R-key/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-mem/read-full-History
```

The first returns every section with its whole text (no heading tree, no levels as a tree); the
second returns one section, but only by its heading text and only as text. Neither yields the
fenced blocks, the links or the table under a heading as records.

Re-checked at commit 18732dd (2026-10-21): Markdown structure is read by a hand-written line
scanner in `liquers-lib/src/search/md_split.rs` (`sections`, `find_section`, `first_heading`,
`section_paragraph`, `blank_front_matter`), now shared by `ns-mem` (`mem/read.rs`, `write.rs`,
`rules.rs`, `tiers.rs`) and the `markdown_tiers` metadata step (`store_config.rs:207`). Links are
scanned privately for `check`'s `dead_links` rule (`mem/check.rs`). No outline, link, code-block or
table-under-heading command exists; `pulldown-cmark` is still only behind `webui`. `-mem-read` with
`section` covers "a section by heading" for memory documents only.

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

## Decisions (maintainer, 2026-10-31)

Proposal: project file `agent-memory-mvp/markdown-commands-proposal.md` (thread "Plan status
check"). A new default-on `markdown` feature in `liquers-lib`. Commands take one document; commands
over a folder may exist beside them. Writing commands, editing and rendering move to
`NO-QUERY-AWARE-MARKDOWN-PARSER`, which also decides between a temporary `pulldown-cmark` and a
Liquers-owned parser. Chosen the same day: our own block layer first, with links and code spans
(`design/markdown-navigation/`).

## Progress (2026-11-01)

The first slice is implemented by `design/markdown-navigation/` (PR #228): `ns-md/outline`,
`section`, `sections`, `links` and `table` on the `liquers-markdown` parser, with
`ns-search/md_split`, `ns-mem` and `markdown_tiers` moved onto it
(`specs/reference/MARKDOWN.md`). The rest of this issue's command set, fenced blocks by language,
front matter as a command, commands over a folder and incoming links, is
`MARKDOWN-CODE-BLOCKS-FRONT-MATTER-AND-BACKLINKS-ARE-NOT-COMMANDS`.

**Resolution:** closed with the first slice (PR #228; tests `liquers-lib/tests/markdown_commands.rs`,
`liquers-markdown`); the remainder is tracked by the issue above.

## Discovery

Raised by the user on 2026-10-11 while reviewing `store-and-asset-search` revision 8, where `md_split`
was accepted as a proof of concept "looking like a need for a more general markdown navigation,
filtering and interpretation tool" (`specs/design/store-and-asset-search/agent-queries.md` §5).
