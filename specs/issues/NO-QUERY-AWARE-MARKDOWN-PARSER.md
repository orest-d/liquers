---
id: NO-QUERY-AWARE-MARKDOWN-PARSER
kind: feature
title: Liquers has no Markdown parser that knows queries and templates and can edit and render a document
status: draft
priority: P2
complexity: XL
area: [lib/commands, records]
design:
created: 2026-10-31
github:
---
# Liquers has no Markdown parser that knows queries and templates and can edit and render a document

## Problem

Markdown is the main document format of a Liquers store used as agent memory, and of template
pages (`index.md`, context packs). Liquers reads it in three unrelated ways, none of which can
serve as the base of `NO-MARKDOWN-NAVIGATION-COMMANDS`:

- a hand-written line scanner, `liquers-lib/src/search/md_split.rs` (`sections`, `find_section`,
  `first_heading`, `first_paragraph`, `section_paragraph`, `blank_front_matter`), shared by
  `ns-mem` (`mem/read.rs`, `write.rs`, `rules.rs`, `tiers.rs`) and `markdown_tiers`. It knows ATX
  headings and backtick or tilde fences only: no setext headings, no lists, links or tables, and
  a heading inside an HTML block or an indented code block counts as a heading;
- a private link scanner for `check`'s `dead_links` (`mem/check.rs`);
- `pulldown-cmark` behind `webui` (`ui/widgets/markdown_element.rs`, Markdown to HTML only) and
  `egui_commonmark` behind `egui` (rendering only); `liquers-records` reads a Markdown table as a
  whole-file format (`formats/markdown.rs`).

None of them knows Liquers syntax. A query in a link target or inline text (`-R/data/a~~b.csv`,
`~X~` links, `_` in keys) is read as Markdown; with strikethrough enabled, `~~` in a query is
strikethrough. A Mustache tag (`{{$ q}}`, `{{#section}}`) is plain text to all of them, so a
template page has no outline, and a value inserted into a table cell or a link is not escaped for
where it lands. None of them gives an editable document: `ns-mem/edit` splices line ranges, and
`pulldown-cmark` yields an event stream that cannot be written back unchanged.

## Impact

- The Markdown commands (outline, section, links, tables) would be built twice: once on a
  temporary parser, again on a real one.
- Writing a section, appending a row or setting front matter is done by text splicing in
  `ns-mem`, with no structure to validate against and no way to reuse it in other commands.
- Templates cannot be checked for the Markdown they produce, and `expand_md` errors and
  inserted values are not escaped for their Markdown context.
- Each output format (HTML for `webui`, egui, plain text for search and tiers, records) is
  produced by a different library, or not at all.

## Expected behaviour

A Liquers-owned Markdown parser (probably its own crate on `liquers-core`, like
`liquers-template`) whose document model the Markdown commands, `ns-mem`, `markdown_tiers`, the
template crate and the renderers share. Requirements collected so far, for discussion:

**Document model**

1. **Lossless.** Every byte of the input belongs to a node, and rendering an unedited document
   back to Markdown gives the input byte for byte. An edit changes only the bytes of the nodes it
   touches, so a write leaves a minimal diff in git.
2. **Positions.** Every node has its byte range and line/column (`liquers_core::query::Position`,
   as the template crate uses), for diagnostics, `line`/`end_line` columns and editing.
3. **Never fails.** Malformed input becomes raw nodes plus diagnostics, as an invalid template tag
   becomes an `Invalid` node.
4. **A section tree over the blocks,** with one address rule (heading text, `A > B` path, number
   prefix `3.2`, slug) shared by every command; an ambiguous address is an error naming the
   candidates.

**Syntax**

5. Blocks: front matter (YAML `---`, TOML `+++`), ATX and setext headings, paragraphs, lists
   (nested, ordered, task items), block quotes, fenced and indented code with info strings, GFM
   tables, thematic breaks, HTML blocks; anything else kept as raw text. Agreeing with CommonMark
   where it matters for structure, checked against its spec tests and differentially against
   `pulldown-cmark` over `specs/`.
6. Inlines: code spans, links (inline, reference, autolinks), images, emphasis, strikethrough.
   Deciding how far inline parsing must go is part of the design; the commands need links and
   code spans first.
7. **Queries recognised:** a link target or a code span that parses as a Liquers query is held as
   a parsed `Query` (with its key and resolved key), and query characters are never read as
   Markdown markup inside it; a fenced block with a `liquers`/`query` info string holds queries.
8. **Mustache tags recognised** as opaque nodes at inline and block level, including section tags
   whose open and close lie in different blocks, so a template page has an outline and can be
   validated without rendering. Whether the Markdown and Mustache grammars are lexed together or
   in two passes is a design question.

**Editing**

9. A convenient editing API on the AST: replace a section's body, insert or remove a section,
   append to a list or a table (a History row), set or remove a front-matter field, rename a
   heading, rewrite link targets (a moved key). `ns-mem/write` and `edit` move onto it.
10. Building a document programmatically (records to a table, a list, a section), so generated
    pages are assembled as structure rather than strings.

**Rendering and formats**

11. Markdown out (identity, and a normalising form), HTML (replacing `pulldown-cmark` in
    `webui`), plain text (search, L0/L1 tiers), records (sections, links, blocks, tables, front
    matter), JSON of the AST; egui and other targets as later renderers over the same model.
12. Context-aware escaping when a value is inserted: table cell, link target, heading, code.

**Engineering**

13. Linear time, cheap enough for `on_read` (`markdown_tiers` runs on every read); no `unsafe`,
    no `unwrap`, no stdout; builds for `wasm32`.

**To analyse in the design**

- **Integration with Mustache:** what it buys (outline and validation of template pages,
  Markdown-aware escaping of inserted values, sections and tables of a parsed document as
  template data, `expand_md` errors placed correctly) against what it costs (two grammars in one
  lexer, sections that cross block boundaries, a template's structure known only after
  rendering). Options: Markdown-aware templates, template-aware Markdown, or two passes.
- **Which formats** can be generated from the model, and which belong to it versus to
  per-format renderers in `liquers-lib`.
- **Temporary `pulldown-cmark`** for the first Markdown commands, or the parser first: see
  `NO-MARKDOWN-NAVIGATION-COMMANDS`.

## Discovery

Raised by the maintainer on 2026-10-31 in project thread "Plan status check", while deciding the
parser for `NO-MARKDOWN-NAVIGATION-COMMANDS` (proposal: project file
`agent-memory-mvp/markdown-commands-proposal.md`). The writing commands of that proposal were
moved here, because they need an editable AST.
