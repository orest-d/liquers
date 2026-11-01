# Phase 1: High-Level Design — User documentation books, published on the web

## Purpose

Give Liquers public user documentation in the form of hand-edited, book-length documents, kept
apart from the agent-maintained topic guides in `specs/guides/`, and publish them, with the
generated API references, as a website on GitHub Pages. A CI pipeline builds and checks every
change and deploys `main`. This serves `VISION.md`'s adoption goals: a newcomer can read Liquers
from a link, in order, without opening the repository.

## Problem Example

A data scientist hears of Liquers and wants to learn to use it from Python. Today they find:

- The repository README.
- `specs/guides/PYTHON_GUIDE.md`, 727 lines, which assumes the codebase and links into
  `specs/reference/`.
- Twenty-one other guides with no reading order.

None of it is on the web.

Should be: `https://orest-d.github.io/liquers/` opens a homepage. From there, *Liquers for Python
users* reads like a book: chapters in order (installing, the first query, stores, commands in
Python, values and data frames, recipes, troubleshooting), every example run in CI, cross-links to
the Python API reference, and a "where next" at the end. A user-facing reader never needs
`specs/`.

The two kinds of document, side by side:

| | `specs/guides/` (kept) | `docs/books/` (new) |
|---|---|---|
| Reader | contributors, coding agents | users of Liquers |
| Shape | one topic, task-oriented, as short as possible | a book: ordered chapters, narrative, worked examples |
| Who edits | agents, with every design that touches the topic (`DOCS_STRUCTURE_GUIDE.md` §9.3) | people, with agent drafts reviewed by a person; not rewritten wholesale by an agent |
| Change rate | with the code | with releases and deliberate revisions |
| Quality bar | true at HEAD, reviewed every 92 days | editorial: style guide, review, executed examples, no broken links |
| Published | no | yes, on GitHub Pages |

## Scope and Acceptance Criteria

- **AC-1** A book is a folder of chapters
  WHEN a `docs/books/<book>/` folder with a `book.yaml` (title, audiences, chapter order) and
  chapter Markdown files is added
  THEN the site shows it on the homepage under its audiences, with a table of contents and
  previous/next navigation between chapters, and no other file is edited
- **AC-2** Books do not depend on `specs/`
  WHEN a chapter links into `specs/` relatively, or by a GitHub URL to `specs/`
  THEN the documentation check fails, naming the chapter and the link
- **AC-3** Book examples run
  WHEN a chapter contains a fenced example marked as runnable (`rust`, `python`, `javascript`, or
  a Liquers query)
  THEN CI executes it (Rust as a doctest, Python with pytest, JavaScript under Node, queries with
  `liquers-validate` or a fixture environment) and fails when it does not run
- **AC-4** The editorial standard is written down
  WHEN someone (a person or an agent) writes or edits a chapter
  THEN `specs/guides/USER_BOOKS_GUIDE.md` tells them the voice, structure and chapter template,
  the example rules, how a book relates to the guides and references it draws on, and that an
  agent changes a book only when asked, in a PR a person reviews
- **AC-5** Every PR builds the site
  WHEN a PR changes `docs/`, `liquers-docs/`, the build script or any doc comment
  THEN CI builds the whole site (API references included), runs the link and example checks, and
  attaches the built site as a downloadable artifact
- **AC-6** `main` is published
  WHEN a change lands on `main`
  THEN CI deploys the built site to GitHub Pages, and the homepage is reachable at the Pages URL
- **AC-7** A first book exists
  WHEN the design is implemented
  THEN at least one book is published with its full table of contents and at least its first
  chapters written and reviewed, so the structure is proven on real content

Non-goals:
- Moving or deleting anything in `specs/guides/`.
- Writing every planned book: each later book is its own issue.
- Versioned documentation per release: kept possible by relative links (question 4), not built.
- Search, comments and analytics.
- A custom domain.

## Core Interactions

- Documentation sources under `docs/books/`.
- The site build of `design/documentation-site-tool/`.
- CI workflows.
- The repository's GitHub Pages setting.
- Example runners per language: Rust doctests, `liquers-py`'s pytest, `liquers-web`'s Node runner.

## Crate Placement

No library change. Book support (book manifests, table of contents, chapter navigation, example
extraction) extends `liquers-docs` from `documentation-site-tool`. The Rust example runner is a
`#[cfg(doctest)]` include of each chapter in `liquers-docs`. CI lives in `.github/workflows/`.

## Documentation Intent

- Reference: none.
- Guide: new `specs/guides/USER_BOOKS_GUIDE.md`, internal: how to write and review a book.
- API docs: `liquers-docs` modules for books, per `API_DOCUMENTATION_GUIDE.md`.
- Other documents: `docs/books/<first book>/`; `docs/README.md` (books section).
- Documents to update:
  - `DOCS_STRUCTURE_GUIDE.md` §9.6: `docs/` holds published books, not documents moved from
    `specs/`; "move, never fork" no longer applies to guides.
  - `CLAUDE.md`: books and how agents may edit them.
  - `LANGUAGE-INTEGRATION_GUIDE.md` §DOCS: an integration's user guide stays in `specs/guides/`,
    and its users' book is in `docs/books/`.
  - `specs/README.md`.

## Open Questions

1. **The first book (open design).** Recommended: *Liquers for Python users*. The Python package
   is the most complete integration, its examples already run (`test_GUIDE_examples.py`), and it
   serves `VISION.md`'s data-science use case. Alternatives: a language-neutral *Liquers: concepts
   and the query language* first, which every later book would then reference; or *Building with
   Liquers in Rust*.
2. **How agents may edit books (open design).** Recommended: an agent edits `docs/books/` only
   when asked to, and every such PR needs a person's review. A `CODEOWNERS` entry
   (`/docs/books/ @orest-d`) makes the review requested automatically. An agent may propose a
   chapter change when a design invalidates it (Phase 5 lists affected chapters). It does not
   apply that change unasked.
3. **Pages deployment (proposed resolution).** GitHub Actions deployment
   (`actions/upload-pages-artifact` and `actions/deploy-pages`) from `main`, with no `gh-pages`
   branch. It needs a one-time repository setting (Pages → Source: GitHub Actions) that only the
   owner can make.
4. **Versions (proposed resolution).** Publish `main` only, at the site root. Every link is
   relative (`api-documentation-rules`), so per-release copies under `/<version>/` can come later
   without rewriting pages. The later step would switch to a `gh-pages` branch that keeps old
   versions.
5. **Which API references CI builds (open design).** Recommended: Rust always. Python and
   JavaScript are added as soon as their generators are chosen (`py-api-reference`,
   `WEB-API-REFERENCE-NOT-GENERATED`). Until then the site says they are not built yet. The CI
   cost of the Python and wasm toolchains is measured in Phase 2.
6. **Is a chapter a template? (open design).** Recommended: chapters are plain Markdown, so they
   are easy to edit by hand. A chapter may use a query tag only for a generated element (a
   command table, a version number), and the site tool's `check` lists every chapter that does.
   Alternative: allow free templating, which makes chapters harder to edit and to review.

## Design Dependencies

- `documentation-site-tool` — **requires**: the site build. This design adds books to it and
  **takes over its CI question 5**: the workflow, artifact and deployment are specified here.
- `api-documentation-rules` — **requires**: `docs/` and its rules. Its `docs/guides/` folder
  becomes `docs/books/`, and its `getting-started/` pages become the first chapters of the books.
  Both are amended at that design's Phase 3. Its question 6 (move the guides) is withdrawn by the
  owner's decision of 2026-11-01.
- `py-api-reference`, `WEB-API-REFERENCE-NOT-GENERATED` — **overlaps** (question 5).

## References

- Owner's request of 2026-11-01 in the project thread; `specs/issues/USER-GUIDES-LIVE-IN-SPECS.md`.
- `liquers-py/tests/test_GUIDE_examples.py`, `liquers-web/tests/guide/run-guide-examples.mjs`: the
  existing executed-example runners.
- `.github/workflows/docs-check.yml`: the existing offline documentation check.
