---
title: User Documentation Editing Guide
kind: guide
audience: internal
area: [docs]
reviewed: 2026-11-01
---
# User Documentation Editing Guide

How to write, check and maintain the public user guides in `docs/guides/`: *Liquers for Rust*,
*Liquers for Python* and *Liquers for the Web*. They are book-like documents for users, edited by
hand and published at <https://orest-d.github.io/liquers/>. The topic guides in `specs/guides/`
stay what they are, agent-maintained notes for people working on Liquers; they are the material the
user guides are written from, not their source files. Design:
[`design/user-documentation-guides/`](../design/user-documentation-guides/DESIGN.md). How the site
is built and deployed: [`GITHUB_ACTIONS_GUIDE.md`](GITHUB_ACTIONS_GUIDE.md).

## 1. The layout

```
docs/
  index.md                    the site's home page
  guides/index.md             how the three guides relate
  guides/_shared/*.md         language-neutral fragments, included by all three guides
  guides/{rust,python,web}/
    guide.yaml                title, language, authors, chapter order
    index.md                  the preface (chapter 0)
    01-introduction.md … 15-troubleshooting.md
    a-query-syntax.md … d-sister-guides.md   appendices
    HISTORY.md                one row per chapter review
  theme/liquers.css           the shared style of every book
```

The three guides have the same chapters with the same numbers and file names, so chapter 5 is
about commands everywhere and a chapter can point to its sister: "the same in Rust:
`[5. Commands](../rust/05-commands.md)`". When a chapter does not apply to a language, it keeps
its number and holds one paragraph saying why and where to look instead. Appendix D maps the
chapters of the other two guides. `liquers-docs/tests/skeleton.rs` fails when the three
`guide.yaml` files list different chapters.

A new chapter is added to all three guides at once, with the same number, and to each
`guide.yaml`. A file that is not listed is not published (`liquers-docs check` warns).

## 2. A chapter

```markdown
---
title: Commands
status: draft
sources: [guides/PYTHON_GUIDE.md, reference/COMMAND_DECLARATION.md]
reviewed:
---
# 5. Commands

A Python function becomes a command with `@command` …
```

- `title`: the chapter's name without its number, as the table of contents shows it.
- `status`: `outline` (skeleton only), `draft` (text written; markers allowed), `review`
  (waiting for a person), `ready` (published by release builds; no `TBD:` or `CHECK:`).
- `sources`: the `specs/` documents (paths relative to `specs/`) the chapter is written from. A
  design whose `affects_docs` names one of them lists this chapter for review (§7).
- `reviewed`: the date the chapter was last checked against its sources, with a row in the
  guide's `HISTORY.md`. Empty until the first review.

The first heading is the chapter's number and title, `# 5. Commands`; appendices are lettered,
`# A. Query syntax`; the preface is `# Preface`. Sections are numbered `## 5.1 …` so that a
reference to "5.2" means the same section in every guide.

## 3. Style

- **Reader.** A user of Liquers, not a contributor. Explain what to do and why; leave out how
  Liquers is built inside, design history and issue IDs.
- **Voice.** Second person, present tense, active voice: "Register the command, then evaluate the
  query." One idea per sentence; paragraphs of two to five sentences.
- **Order.** Teach in the order a reader needs it: a short example first, then the explanation,
  then the variations and the rules.
- **Length.** A chapter is read in one sitting: aim for 1,500-4,000 words. Move reference
  material to an appendix or to the API reference and link it.
- **Headings.** Sentence case, numbered (`## 5.2 Arguments and defaults`), no more than three
  levels below the title.
- **Code.** Every code block names its language (` ```rust `, ` ```python `, ` ```js `,
  ` ```text ` for queries). An example is complete enough to run, or says what it leaves out.
  Queries are checked with `liquers-validate` before they go in (`CLAUDE.md`, *Validating
  queries*).
- **Terms.** The words of appendix C, used the same way in all three guides. Introduce a term in
  *italics* where it is first defined.
- **Links.** Relative links between chapters and guides. Never a link into `specs/`: it is not
  published, and `docs_index.py --check` (check 18) and `liquers-docs check` refuse it. Link the
  published API reference or an external page by URL instead.

## 4. Editorial markers

A line that starts with one of these words, in capitals and followed by a colon, is a note to
writers, not reader text. The note runs to the next blank line, heading or marker.

| Marker | Meaning | Draft build | Release build |
|---|---|---|---|
| `TBD:` | content still to be written; says what | shown as a quoted note | fails in a `ready` chapter |
| `PURPOSE:` | what the chapter enables | shown | removed |
| `SOURCE:` | where to draft from (`specs/` path and section) | shown | removed |
| `EDITOR:` | a note or instruction to editors | shown | removed |
| `CHECK:` | a claim to verify against the code before release | shown | fails in a `ready` chapter |

A longer note starts each of its paragraphs with the marker. Markers inside code blocks are text.
A release build publishes only `ready` chapters; every other chapter appears as its title and
"This chapter is in preparation", and its table-of-contents entry says so. `liquers-docs report`
lists every chapter's status and open markers.

## 5. Includes and generated content

Every chapter is a Liquers template ([`TEMPLATE_GUIDE.md`](TEMPLATE_GUIDE.md)), expanded by
`liquers-docs` with `docs/` and `specs/` mounted read-only. Three kinds of inclusion are used:

- **A shared fragment**, for an explanation that is the same in every guide:
  `{{> ../_shared/queries.md}}`. Fragments have no front matter; their markers count against the
  chapters that include them.
- **A section of a `specs/` document**, for wording that must stay identical to the
  specification: `{{$ -R/specs/reference/PROJECT_OVERVIEW.md/-/ns-docs/include-Glossary}}`.
  `ns-docs/include` returns the section's body with its subsections, without its own heading. The
  heading matches with or without its number, case-insensitively; a space in it is written `~.`
  (`include-Argument~.defaults`).
- **Generated content**, such as the command library of appendix B:
  `{{$ -R/specs/command_registry.yaml/-/ns-docs/command_table}}`.

When to include rather than write:

- **Include** text that must not drift: a syntax table, an option list, a signature, the glossary.
- **Write** narrative, explanation and examples. They are written for the reader of this guide,
  and a specification's wording is written for a contributor.
- **Never include** a whole `specs/` document into a chapter's prose; only an appendix may take a
  whole document, and only one written to stand alone.

The rules an included section must meet, checked on every build:

- the heading exists (renaming it fails the docs build of the pull request that renamed it,
  because `docs-site.yml` also runs on `specs/**` changes);
- it holds no relative link, because the link would break once the text is moved; give the
  section absolute links, or split out a link-free subsection to include;
- it holds no editorial markers.

An include is a recorded dependency of the chapter, so it is listed in the chapter's `sources:`
too. Use `{{$ }}` only, never `{{$$ }}`: the build has no payload.

## 6. Tool-neutral chapters

Chapters are CommonMark with tables and footnotes, plus Liquers templates. Nothing that only one
renderer understands: no `{{#include}}` or other mdBook helpers, no `::: div` blocks, no
`{.class}` attributes, no `@ref` cross-references, no MyST directives or roles. Structure lives in
`guide.yaml`; `SUMMARY.md` and `book.toml` are generated. `liquers-docs check` refuses renderer
syntax, so the renderer can be changed without touching a chapter.

## 7. Maintenance

Chapters are maintained like the other documents under `specs/DOCS_STRUCTURE_GUIDE.md`:

- `scripts/docs_index.py` indexes every chapter (kind `chapter`, id `<guide>/<file stem>`). Its
  `--check` verifies the status, that each `sources:` entry exists, that a `reviewed:` date has a
  row in `HISTORY.md`, and that no chapter is more than 92 days past its review.
- **After a design changes a specification**, Phase 5 of the `liquers-project` workflow runs
  `python3 scripts/docs_index.py chapters-for <affects_docs entries>` and reviews every chapter
  it lists. A chapter that is still right gets a new `reviewed:` date and a `HISTORY.md` row; one
  that is wrong is fixed in the same pull request, or gets a `CHECK:` marker and an issue in
  `specs/issues/`.
- **A review** checks the chapter's claims and examples against its sources and the code. Record
  it as one `HISTORY.md` row: `| 2026-11-01 | 05-commands.md | reviewed against … |`, and bump
  `reviewed:`.
- **Who edits.** Anyone, through a pull request; changes to `docs/guides/` request a review from
  the owner (`.github/CODEOWNERS`). Agents edit a chapter only when asked or within a design.

## 8. Building and previewing

```bash
cargo run -p liquers-docs -- check            # every finding; exit 1 on an error
cargo run -p liquers-docs -- check --release  # also fails on TBD:/CHECK: in ready chapters
cargo run -p liquers-docs -- report           # status, review date and open markers per chapter
bash scripts/build-docs.sh --skip-api         # the site in target/docs-site
```

`build-docs.sh` needs `mdbook`, `pandoc` and, for PDFs, `typst` (`--latex` uses `xelatex`
instead); a missing tool skips its step with a notice. Open `target/docs-site/index.html` in a
browser to read the result. Every pull request also uploads the draft site as the `docs-site`
artifact of its `docs-site` run.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | Created: layout, chapter format, style, markers, includes, tool-neutral rule, maintenance, building. | `design/user-documentation-guides` (phase-5) |
