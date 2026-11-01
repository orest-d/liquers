# Phase 1: High-Level Design — User guides, their formats, publishing and CI

Revision 3, 2026-11-01. It follows the owner's reviews the same day: the folder is `docs/guides/`;
`specs/guides/` stays and is the basis the public guides are written from; the decisions below,
including mdBook + Pandoc as renderers. Supporting documents: [`guide-outlines.md`](guide-outlines.md)
(question 1) and [`tooling.md`](tooling.md) (questions 5 and 6).

## Purpose

Give Liquers three public user guides: *Liquers for Rust*, *for Python* and *for the Web*. They
are book-like, hand-edited, parallel in structure and held to a high standard. They are rendered
as HTML with and without embedded search, EPUB and PDF, built and checked by CI on every change,
and published to GitHub Pages from `main`. `main` holds only sources. This serves `VISION.md`'s
adoption goals: a newcomer learns Liquers from a link, in order, in their language.

## Problem Example

A data scientist wants to use Liquers from Python. Today they find the README and
`specs/guides/PYTHON_GUIDE.md`. That guide is 727 lines, written for agents and contributors, and
links into `specs/reference/`. The other twenty-one topic guides have no reading order. Nothing is
on the web, and nothing comes as EPUB or PDF.

Should be: `https://orest-d.github.io/liquers/` links three guides with the same chapters (see
[`guide-outlines.md`](guide-outlines.md) §2). *Liquers for Python*, chapter 5 *Commands*, teaches
`@command` and links to chapter 5 of the Rust and Web guides for the same topic. Its examples run
in CI. Its syntax table is included from `specs/` rather than copied. The same guide downloads as
EPUB or PDF.

`specs/guides/` stays as it is: short, topic-specific, agent-maintained, true at HEAD. A public
guide is written *from* it, typically one or more topic guides per chapter, reorganised and
rewritten for a reader rather than moved. Where the wording must stay identical (a syntax table,
an option list), the chapter includes the `specs/` section through a template, so nothing is
duplicated.

## Scope and Acceptance Criteria

- **AC-1** A skeleton of three parallel guides
  WHEN `docs/guides/` is opened
  THEN it holds `rust/`, `python/` and `web/`, each with a `guide.yaml` and the same numbered
  chapter files, each chapter in `status: outline` with its `PURPOSE:`, a `TBD:` content proposal
  per section, and its `SOURCE:` and `sources:` front matter, as `guide-outlines.md` specifies
- **AC-2** Editorial markers never reach readers unintentionally
  WHEN a release build meets `TBD:` or `CHECK:` in a `ready` chapter
  THEN it fails naming the chapter; `PURPOSE:`, `SOURCE:` and `EDITOR:` are stripped from release
  output and shown muted in draft output
- **AC-3** The editing guide exists
  WHEN anyone edits `docs/guides/`
  THEN `specs/guides/USER_DOCS_EDITING_GUIDE.md` gives the style, structure, markers, statuses,
  cross-guide referencing, example rules, the include rules (when to include from `specs/` and
  how), and the maintenance rules
- **AC-4** Guides are maintained like other documentation
  WHEN a chapter exists
  THEN it carries `reviewed:` and `sources:`, its guide's `HISTORY.md` records each review, it is
  in `specs/index.csv` and the 92-day review, and a design whose `affects_docs` names one of its
  `sources` lists the chapter for review at Phase 5
- **AC-5** Includes from `specs/` are safe
  WHEN a chapter includes a `specs/` document or a section by heading
  THEN the build fails if the section is missing or contains a relative link, and the chapter is
  recorded as depending on it
- **AC-6** All formats from one source
  WHEN the build script runs
  THEN it writes, per guide, HTML with search, HTML without search, EPUB and PDF (Typst; LaTeX as
  an option), plus the homepage linking the guides and the API references
- **AC-7** CI checks every PR and publishes `main`
  WHEN a PR changes `docs/`, the tool or the build script
  THEN CI runs the preprocessing checks and a draft build and attaches the outputs as an
  artifact; WHEN a change lands on `main` THEN CI builds a release and deploys it to GitHub Pages
  through Actions, and no built file is committed to any branch of the repository's source
- **AC-8** The workflows are documented
  WHEN the design is implemented
  THEN `specs/guides/GITHUB_ACTIONS_GUIDE.md` describes every workflow (triggers, jobs, permissions,
  how to add one) and reserves sections for publishing `liquers-py` to PyPI and `liquers-web` to
  npm or a CDN

Non-goals: writing the guides' content beyond the skeleton (each guide's chapters are follow-up
issues); moving or changing `specs/guides/`; versioned per-release sites; the PyPI and npm
pipelines themselves (only their place in the Actions guide); a custom domain.

## Core Interactions

Templates (`ns-template`, partials, query tags) and section reads (`ns-mem/read`) over `specs/`
for the preprocessing, in a small Rust tool; mdBook for the HTML site and Pandoc for EPUB and PDF;
CI workflows and GitHub Pages; the API references from `api-documentation-rules`.

## Crate Placement

No library change. The preprocessing tool is the `liquers-docs` crate (`documentation-site-tool`,
reshaped to stage 1 only): a binary over `liquers-lib` with `templates` and `mem`, so whole-file
and section includes need no new command. It is a workspace member outside `default-members`. The
build script is `scripts/build-docs.sh`; the workflows are in `.github/workflows/`.

## Documentation Intent

- Reference: none.
- Guides: new `specs/guides/USER_DOCS_EDITING_GUIDE.md` and `specs/guides/GITHUB_ACTIONS_GUIDE.md`
  (both internal).
- API docs: the `liquers-docs` modules, per `API_DOCUMENTATION_GUIDE.md`.
- Other documents: the `docs/guides/` skeleton; `docs/README.md` (building the docs).
- Documents to update: `DOCS_STRUCTURE_GUIDE.md` (§2, §4.2 a `chapter` kind, §9.6 rewritten:
  `specs/guides/` is the basis of `docs/guides/`, nothing moves); `CLAUDE.md`;
  `LANGUAGE-INTEGRATION_GUIDE.md` §DOCS (an integration's user guide is its chapter set in
  `docs/guides/<language>/`); `specs/README.md`.

## Decisions taken (owner, 2026-11-01)

1. Three guides, Rust, Python and Web, with very similar structure so they can reference each
   other; the old LiQuer docs as a model; existing guides as the drafting basis; only structure and
   content proposals for now; a marker convention for instructions and comments.
2. An editing guide with style, rules and conventions; guides maintained like other documentation;
   `specs/` sections or documents may be included through templates, with rules for when.
3. A guide (or guides) describing the GitHub Actions, later covering PyPI and npm or CDN publishing.
4. GitHub Pages through Actions; `main` holds only sources.
5. Evaluate the renderer options (look, integration, consistent appearance, search, maturity).
6. Chapters are templates; propose what the tool must do; consider the agent memory system or a
   thin `liquers-py` wrapper.
7. Renderer: **mdBook** for the HTML site (with and without search) and **Pandoc** for EPUB and
   PDF, fed by a Liquers preprocessing step. Chapters stay **tool-neutral**: CommonMark, Liquers
   templates, relative links, our markers, structure in `guide.yaml`; no mdBook, Pandoc, MyST or
   Quarto syntax in a chapter. A Liquers-based documentation tool (liquers-doc) is the long-term
   direction, reached step by step without changing a chapter (`tooling.md` §5-6).

## Open Questions

1. **How Liquers hands chapters to mdBook (open design).** Recommended: the tool expands every
   guide into a build tree of plain Markdown with a generated `SUMMARY.md` and `book.toml`, which
   mdBook and Pandoc both read. Using mdBook's preprocessor protocol (the book as JSON on stdin)
   is the next step towards liquers-doc, not the first one: a build tree serves both renderers and
   is easy to inspect.
2. **PDF engine (proposed resolution).** Typst by default (a single binary, fast); LaTeX through a
   Pandoc option for print-quality output.
3. **Who may edit (proposed resolution).** Anyone, through a PR. An agent edits a chapter only
   when asked or within a design that lists it, and a person reviews every chapter that reaches
   `ready`.
4. **Search across the three guides (proposed resolution).** mdBook searches one book at a time;
   accepted for now. A cross-guide search is a liquers-doc feature (`tooling.md` §6).

## Design Dependencies

- `api-documentation-rules` — **requires** its `docs/` root and link rules; its `docs/guides/`
  layout is replaced by `guide-outlines.md`, its `getting-started/` pages become chapters 1-2.
- `documentation-site-tool` — **overlaps**: its `liquers-docs` crate is reshaped into the
  preprocessing tool (no Markdown-to-HTML, no page layout; mdBook and Pandoc render), and its CI
  question moves here. Revised at its Phase 3.
- `py-api-reference`, `WEB-API-REFERENCE-NOT-GENERATED` — **overlaps**: the API references the
  homepage links.

## References

- `specs/issues/USER-GUIDES-LIVE-IN-SPECS.md`; the owner's messages of 2026-11-01.
- <https://orest-d.github.io/liquer/site/index.html> (the old LiQuer user guide).
- `liquers-py/tests/test_GUIDE_examples.py`, `liquers-web/tests/guide/run-guide-examples.mjs`.
