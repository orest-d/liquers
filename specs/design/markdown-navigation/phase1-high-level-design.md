# Phase 1: High-Level Design — Markdown navigation on a Liquers block parser

## Purpose

Agents using a Liquers store as memory find a document and then read all of it, because nothing
can return its outline, one section, its links or the table under a heading. This design adds
general `ns-md` commands for that, built on the first layer of a Liquers-owned Markdown parser: a
lossless block layer that knows where queries are. It is the first slice of
`NO-MARKDOWN-NAVIGATION-COMMANDS` and of `NO-QUERY-AWARE-MARKDOWN-PARSER`, and it serves the agent
memory, the first application of the agent workbench (`specs/reference/VISION.md` §The agent
workbench).

## Problem Example

An agent needs §8 *Storage* of `specs/reference/AGENT_MEMORY_SERVICE.md` and the documents it links
to. Today the nearest queries are

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-search/md_split-section
-R-key/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-mem/read-full-Storage
```

The first returns every section with its whole text; the second works only for memory documents,
only by exact heading text, and returns text. Neither lists links. The line scanner behind both
(`liquers-lib/src/search/md_split.rs`) misses setext headings and counts a `#` line inside an HTML
block or an indented code block as a heading.

With this design:

```
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/outline
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/section-8
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/links
-R/specs/reference/ASSETS.md/-/ns-md/table-History
-R/specs/design/runtime-volatility/DESIGN.md/-/ns-md/section-phase~.2.solution
```

The outline is a few hundred bytes of records (level, heading, number, line range, size). The
agent picks `8`, reads only that section, and takes the `links` rows of the section to follow.
All five queries were checked with `liquers-validate` (`--command md/<name>`). `8` is a section
number: the 8th section under the title. The last is a path: the section whose heading contains
"solution" inside the one containing "phase 2" (`~.` is a space in a query).

## Scope and Acceptance Criteria

- **AC-1** Outline
  WHEN `ns-md/outline` is applied to a Markdown document
  THEN it returns one record per heading, in order: `n`, `level`, `heading`, `path`, `number`
  (from a `## 3.2 …` prefix, else empty), `slug`, `line`, `end_line`, `words`; ATX and setext
  headings both count, and `#` lines inside fenced, indented or HTML blocks do not
- **AC-2** Section by address
  WHEN `ns-md/section-<address>` is applied
  THEN it returns that section's text, its subsections included unless `children=false`, its
  heading line included unless `heading=false`; the address is a path of steps separated by `.`,
  each step choosing among the subsections of the previous one (the first, among the sections
  under a lone top-level title)
- **AC-11** Address steps
  WHEN a step consists of digits only
  THEN it is a section number: `1.2.3` is the 3rd subsection of the 2nd subsection of section 1,
  counted in document order; any other step is text, matched case-insensitively against the
  heading's words, exact before prefix before contained, so `phase~.1` does not match "Phase 10";
  `..` is a literal dot, so `3..2` is the text "3.2" and matches `## Section 3.2`
- **AC-12** Addresses in the outline
  WHEN `ns-md/outline` lists a heading
  THEN its record carries `address`, the section-number path (`2.1`) that AC-2 resolves back to it
- **AC-3** Ambiguous or missing address
  WHEN an address matches no section, or more than one
  THEN the command fails with an error naming the address and, for several, every candidate with
  its path and line
- **AC-4** Sections as records
  WHEN `ns-md/sections` is applied
  THEN it returns one record per section with the outline's fields and its `text`; `level=N` keeps
  sections of that level; `ns-search/md_split-section` returns the same rows as today, now from
  the block layer
- **AC-5** Links
  WHEN `ns-md/links` is applied to a document stored under a key
  THEN it returns one record per link and image: `text`, `target`, `kind` (`relative`, `anchor`,
  `url`, `query`), `key` (the target resolved against the document's key, when it is one),
  `section`, `line`; `ns-mem/check`'s `dead_links` findings are unchanged when it reads them,
  except that reference-style links (`[x][ref]`), which the old scanner skipped, are now checked too
- **AC-6** Table under a heading
  WHEN `ns-md/table-<address…>` is applied
  THEN it returns the first Markdown table in that section (`n=` for a later one) as a record
  view, read as the records Markdown table format reads a table
- **AC-7** Lossless block layer
  WHEN any Markdown document under `specs/` is parsed and written back with no edit
  THEN the output equals the input byte for byte, and every block knows its byte range and lines
- **AC-8** Queries are not Markdown
  WHEN a link target or a code span holds a Liquers query (`-R/data/a~~b.csv/-/to_text`)
  THEN the query text is kept intact (`~~`, `_` and `*` read as query characters, not markup)
  and a link whose target parses as a query has `kind = query`
- **AC-9** One parser for the memory
  WHEN `ns-mem` (`read`, `search`, `check`, `edit`) and the `markdown_tiers` store step run
  THEN they find sections, headings and paragraphs through the block layer; their existing tests
  pass unchanged, and the two scanner defects of the problem example are fixed for them too
- **AC-10** A feature of its own
  WHEN `liquers-lib` is built without its new `markdown` feature
  THEN the `ns-md` commands are absent and the build succeeds; the feature is on by default

Non-goals: inline parsing beyond links, images, autolinks and code spans (emphasis, HTML inlines);
rendering to HTML or replacing `pulldown-cmark` in `webui`; the editing API and writing commands;
Mustache-aware parsing (a tag stays text); fenced blocks and front matter as commands (second
slice); new commands over a folder (open question 3).

## Core Interactions

- Commands: new `ns-md`; `ns-search/md_split` and `ns-mem` move onto it (registry regenerated).
- Query: queries recognised with `liquers-core`'s parser (AC-8); link targets resolved to keys (AC-5).
- Values and store: records through the existing `RecordView` glue; `markdown_tiers` (AC-9).

## Crate Placement

The block layer goes in a **new crate, `liquers-markdown`**, depending on `liquers-core` only, like
`liquers-template`. The template crate needs it later for Markdown-aware templates
(`NO-QUERY-AWARE-MARKDOWN-PARSER`), and the editing API and renderers grow on it there. The
commands, the records output and the moved `ns-mem` and `markdown_tiers` code stay in
`liquers-lib` under a new default-on `markdown` feature, which needs `records`.

## Documentation Intent

- Reference: new `specs/reference/MARKDOWN.md` — the block model, the section address rule, the
  `ns-md` commands and their fields; the agent-facing source of truth for structure queries.
- Guide: none; extend `specs/reference/AGENT_MEMORY_SERVICE.md` with navigating a memory document.
- Documents to update: `CLAUDE.md` (project structure, where code goes, feature list),
  `specs/reference/PROJECT_OVERVIEW.md` (crate list), `specs/README.md`, `liquers-agent`'s query
  guide if it names `md_split`.

## Open Questions

Decided by the maintainer on 2026-11-01: a new `liquers-markdown` crate; folder commands later
(`md_split` keeps its folder input); tables through `liquers-records`' Markdown table reader;
`ns-search/md_split-section` stays as a thin wrapper over `ns-md/sections`, `unit=line` as it is;
the path syntax of AC-2 and AC-11.

1. **Step scope:** a step matches direct subsections only (recommended, with an error that
   suggests the full path when a deeper section would match) or any section below.
2. **Lone title:** a single top-level `#` heading is skipped, so `1` is the first `##`
   (recommended).

A section number changes when a section is inserted before it, so stored references (templates,
recipes) should use text steps; the reference will say so.

## Design Dependencies

- Implements the first slice of `NO-MARKDOWN-NAVIGATION-COMMANDS` and the block layer of
  `NO-QUERY-AWARE-MARKDOWN-PARSER`; that issue stays open for inline parsing, editing, rendering
  and Mustache integration.
- Required by `agent-memory-derived` (context packs assemble sections across documents).

## References

`liquers-lib/src/search/md_split.rs`, `liquers-lib/src/mem/`, `liquers-records/src/formats/markdown.rs`;
project file `agent-memory-mvp/markdown-commands-proposal.md` (proposal and decisions, 2026-10-31).
