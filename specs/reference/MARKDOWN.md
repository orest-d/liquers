---
title: Markdown Navigation
kind: reference
audience: internal
area: [lib/commands, records]
reviewed: 2026-11-01
---
# Markdown Navigation

Liquers reads Markdown through its own parser, the `liquers-markdown` crate (it depends on
`liquers-core` only and builds for wasm32). The `ns-md` commands of `liquers-lib` expose it,
behind the `markdown` feature (on by default; it needs `records`). The same parser serves
`ns-search/md_split`, `ns-mem` (`read`, `search`, `check`, `edit`) and the `markdown_tiers` store
step. Why it is shaped this way is [`design/markdown-navigation/`](../design/markdown-navigation/DESIGN.md);
what the parser does not do yet is `issues/NO-QUERY-AWARE-MARKDOWN-PARSER.md`.

## 1. The block model

`Document::parse(text)` never fails. It splits the text into a flat list of blocks that **tiles**
it: every byte belongs to exactly one block, so the blocks concatenated give the input back byte
for byte. Each block has a `Span`: byte range and first and last line (1-based, inclusive).

| Block | Rule |
|---|---|
| `FrontMatter(Yaml \| Toml)` | only at byte 0 (after a BOM): `---` closed by `---` or `...`, `+++` closed by `+++`; the same rule as `liquers_records::search::front_matter_end` |
| `Heading { level, setext, text }` | ATX `#`–`######` followed by a space or the line end, up to 3 spaces of indent, a closing `#` run dropped; setext: a paragraph over a `===` (level 1) or `---` (level 2) line |
| `FencedCode { info }` | 3 or more backticks or tildes, closed by a fence of the same character at least as long, or by the end of the text |
| `IndentedCode` | 4 or more spaces; it cannot interrupt a paragraph |
| `Html` | `<!--` (to `-->`), a CommonMark block tag (`<div`, `<details`, …) or a line that is one complete tag, to a blank line; an autolink `<https://…>` or prose `<T>` is not HTML |
| `Table` | a pipe line followed by a delimiter row with as many cells, to a blank line; it may take a paragraph's last line as its header |
| `List`, `BlockQuote` | a whole list or quote, nested items included; a list continues past blank lines when the next line is indented or an item |
| `ThematicBreak`, `Paragraph`, `Blank` | as CommonMark; consecutive blank lines are one block |

So a `#` line inside a fence, an indented block, an HTML block or the front matter is not a
heading, and setext headings are.

## 2. Sections

Every heading starts a `Section`, which runs to the next heading of the same or a higher level
(its subsections included); its own text ends at its first subsection. A section has its
`level`, `heading` (lines joined by a space), `number` (`8` of `## 8. Storage`, `3.2` of
`## 3.2 Foo`), `path` (the headings from the top), `slug` (GitHub's anchor rule, made unique with
`-1`, `-2`), `parent`, `children`, and `address`.

The top-level sections are those with no enclosing heading, whatever their level. **No heading is
skipped**: a document's `#` title is section `1`, and its `##` sections are `1.1`, `1.2`, ….

## 3. Addresses

An address is a path of steps separated by `.`; each step chooses among the **direct**
subsections of the previous one (the first among the top-level sections).

| Step | Meaning |
|---|---|
| digits only (`8`) | a section number: the 8th subsection, counted from 1 in document order |
| `_` | any one section at that level |
| anything else (`storage`, `phase 2`) | heading text, matched without case on whole words: exact (the heading, or the heading without its number) before prefix before contained; `phase 1` never matches "Phase 10" |

`..` is a literal dot (`3..2` is the text "3.2", matching `## Section 3.2`) and `__` a literal
underscore (`my__var` is "my_var"). Any other `_` inside a step is reserved and refused, so it can
gain a meaning later. `*` cannot be used: the query parser refuses it in a parameter. In a query a
space is `~.`, a `-` is `~_`.

An address must resolve to **exactly one** section. Otherwise the error names the address, the
step that failed and either every candidate (address, heading, line) or, when the step matches
nothing among the direct subsections but a deeper section would, that section's full address.

Section numbers change when a section is inserted before them. A stored reference (a template, a
recipe) should use text steps (`_.storage`), not numbers (`1.8`).

## 4. Links and code spans

Inline parsing finds code spans, inline links and images (`[t](target "title")`, `![a](p)`),
reference links (`[t][ref]`, `[t][]`, `[t]` with a `[ref]: target` definition) and autolinks
(`<scheme:…>`), in paragraphs, lists, quotes, tables and headings, never inside a code span.
Emphasis is not parsed, so query characters (`~~`, `_`, `*`) are never markup.

| `target_kind` | Target |
|---|---|
| `anchor` | `#part` |
| `query` | begins with `-` or contains `/-/`, and `parse_query` accepts it |
| `url` | a scheme (`https:`, `mailto:`) or a leading `/` |
| `relative` | anything else; `anchor` holds the part after `#` |

`Link::resolve(document_key)` resolves a relative target against the document's key:
`Resolved::Key(key)`, or `Resolved::OutsideRoot` when `../` climbs above the store's root.
`ns-mem/check`'s `dead_links` checks every relative link this way (an `OutsideRoot` link is dead).

## 5. Commands (`ns-md`)

Each takes a Markdown document as its input text: `-R/<key>/-/ns-md/<command>`.

| Command | Result |
|---|---|
| `outline` | records `n`, `level`, `heading`, `address`, `number`, `path` (`A > B`), `slug`, `line`, `end_line`, `words`, one per heading |
| `section-<address>-<children>-<heading>` | text of one section; `children=false` drops its subsections, `heading=false` its heading block (both default `true`) |
| `sections-<address>-<level>` | the outline's records plus `text`, for the whole document (`address` empty) or one section and its subsections; `level=N` keeps that level |
| `links-<address>` | records `kind` (`inline`, `reference`, `autolink`, `image`), `text`, `target`, `target_kind`, `anchor`, `key` (resolved against the input's key), `section` (address), `line` |
| `table-<address>-<n>` | the `n`-th table (default 1) inside a section, read as the records Markdown table format reads it (column names lower-cased, labels kept) |

```text
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/outline
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/section-_.storage
-R/specs/reference/AGENT_MEMORY_SERVICE.md/-/ns-md/links-_.8
-R/specs/reference/ASSETS.md/-/ns-md/table-_.history
```

`ns-search/md_split-section` keeps its rows (`heading` is the `A > B` path, a preamble row for
text before the first heading) and its folder input; it now reads them from the parser.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | New: the block model, sections, addresses, links and the `ns-md` commands. | `design/markdown-navigation/` |
