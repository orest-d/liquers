# Rendering tools, formats and the Liquers preprocessing tool

Part of Phase 1 of `user-documentation-guides` (the owner's questions 5 and 6, 2026-11-01).

**Decided 2026-11-01 (owner):** mdBook renders the HTML site and Pandoc renders EPUB and PDF, from
the same Markdown that a Rust `liquers-docs` tool produces; chapters stay tool-neutral (§5); a
Liquers-based documentation tool is the long-term direction (§6). The Quarto recommendation and
option (b) below are kept as the record of the evaluation; they were not chosen. The owner found
Quarto notebook-oriented in look and community, and preferred a Rust-oriented toolchain that
Liquers can grow into.
Tool facts come from the projects' own pages and a March 2026 survey, checked 2026-11-01 (sources
at the end). Phase 2 re-checks versions before anything is pinned.

## 1. Two stages, so the renderer is replaceable

1. **Liquers preprocessing.** Every chapter is a Mustache template. It is expanded with query
   tags into plain Markdown. This step performs `specs/` includes and generated tables and
   resolves the shared fragments. It also checks markers, links and `sources:`, and writes a
   build tree and a manifest.
2. **Rendering.** A standard tool turns that Markdown into each output format.

Stage 1 is the only Liquers-specific part. Stage 2 is chosen for its output formats and can be
swapped without touching a chapter. The requested formats are HTML with an embedded search engine,
HTML without one, EPUB and PDF (via LaTeX or Typst).

## 2. Renderers compared

| | Quarto | mdBook | Sphinx + MyST | Zensical / Material for MkDocs | Pandoc alone | Liquers' own (`documentation-site-tool`) |
|---|---|---|---|---|---|---|
| HTML book with navigation | native (book project) | native | native | native | single pages; navigation hand-made | to build |
| Embedded offline search | native; can be turned off | native (elasticlunr); can be turned off | native | native | none | to build |
| EPUB | native | third-party `mdbook-epub` | native | none | native | none |
| PDF via LaTeX | native | none | native | none (plugins) | native | none |
| PDF via Typst | native | third-party (`mdbook-typst`) | third-party only | none | native | none |
| Look | clean, Bootstrap themes, SCSS | plain, the Rust-book look | many themes (Furo, PyData) | the most polished | template only | ours to make |
| One look across the three guides | one shared SCSS and `_quarto.yml` profile | shared theme folder | shared `conf.py` | shared `mkdocs.yml` base | one template | yes |
| Fit with the API references | links to them; no shared look | closest to rustdoc's look | best for Python (autodoc), weak for Rust | links | links | links |
| Executes examples | Python through Jupyter at render time | Rust (`mdbook test`) | doctest extension (Python) | none | none | via Liquers |
| Integration with stage 1 | reads plain Markdown + `_quarto.yml` | reads `SUMMARY.md` + Markdown | reads MyST Markdown | reads Markdown | reads Markdown | is stage 1 |
| Install in CI | one binary (`quarto-dev/quarto-actions/setup`) | `cargo install` or binary | `pip` | `pip` | binary | built in the workspace |
| Maturity and maintenance | v1.8 (Oct 2025), backed by Posit | v0.5 (2025), rust-lang org, active | 20 years, very active | Material in maintenance mode since early 2026 (fixes for at least 12 months); Zensical is its young successor | v3.9 (Feb 2026), very active | none yet |

Not considered further:
- **Docusaurus.** No EPUB or PDF, and a Node toolchain.
- **Typst alone.** PDF only; its HTML export is experimental.
- **HonKit.** Development has slowed.

**Recommendation at the time (not chosen): Quarto.** It is the only tool with all four formats natively, from the same
sources, with search that can be switched off per build. It also gives one theme across three
books, an active maintainer and a single-binary install. It can execute the Python guide's examples
through Jupyter against the real `liquers` package. Rust examples run as doctests, and Web
examples through the existing Node runner.

The costs:
- Its look is good but not Material's.
- The API references (rustdoc, pdoc, TypeDoc) keep their own looks, as with any choice. The
  navbar links them and keeps the homepage consistent.

Runner-up: **mdBook**, for a Rust-only toolchain and a look close to rustdoc. EPUB and PDF then
rest on third-party plugins.

How the formats map, per guide:
- One Quarto *book* project per guide.
- *HTML with search* is the website.
- *HTML without search* is the same project with `search: false` under a second profile, for
  offline or embedded copies.
- `epub` is a format of the same project.
- PDF comes from the `typst` format by default, and from the LaTeX `pdf` format through a profile.

The homepage and the guides index are a small Quarto *website* that links the three books and the
API references.

## 3. What the preprocessing tool must do

1. Walk `docs/guides/`, in `guide.yaml` chapter order.
2. Expand each chapter as a template: partials from `_shared/`, includes from `specs/` (a whole
   document, or a section by heading), and generated content (the command library from the
   registry, version numbers).
3. Apply the include rules of `USER_DOCS_EDITING_GUIDE.md`:
   - an included section must exist and must contain no relative link;
   - the include is recorded, so the chapter is listed as depending on that source.
4. Handle the markers:
   - in a draft build, leave them for the renderer to style;
   - in a release build, strip `PURPOSE`, `SOURCE` and `EDITOR`;
   - fail on `TBD` or `CHECK` in a `ready` chapter;
   - in a release build, replace a chapter that is not `ready` with its "in preparation" entry.
5. Check the chapters: no link into `specs/`; every relative link resolves inside the build tree.
   The front matter (`status`, `sources`, `reviewed`) must be well-formed.
6. Write the build tree: the expanded Markdown, plus a generated `_quarto.yml` per guide from
   `guide.yaml` and the shared profile.
7. Report: the marker list per guide, the chapters per status, the `sources` map (chapter → specs),
   and overdue reviews.
8. Exit non-zero on any failure, naming the chapter and position.

The renderer then runs: `quarto render build/guides/<guide> --profile html|html-nosearch|epub|pdf`.

## 4. Implementing the tool: three options

| | (a) `liquers-agent` CLI, now | (b) a thin Python wrapper over `liquers-py` | (c) the Rust crate `liquers-docs` |
|---|---|---|---|
| What exists | `liquers-agent query` evaluates any allowed query and prints it. `ns-mem/read` returns a document, a section by heading or a line range. `ns-template` is registered. `memory/liquers-specs.yaml` already mounts `specs/` | `liquers-py` has `templates`, `records` and the store factories. Python walks folders, writes files, calls Quarto, and runs Jupyter for the examples | the full Rust API |
| Missing | No file output: one CLI call per chapter, redirected by a shell loop. The query allow-list has no `template` (a new tool-set file adds it). Each call rebuilds the environment. No manifest or `_quarto.yml` generation. No marker report. Every step 3-7 would be shell script | `liquers-py` is built without `mem`, so there is no section include. Either add `mem` to its features or add a section extractor to `ns-template`. Steps 1, 3-7 are ~300 lines of Python | Everything, ~1,500 lines; `documentation-site-tool`'s Phase 2, reshaped as stage 1 only |
| Build cost in CI | builds `liquers-agent` (needs Rust 1.96 for `s3s`) | builds the `liquers-py` wheel (already done by `liquers-py-tests.yml`), which Quarto needs anyway for the Python examples | builds `liquers-lib` with `templates` |
| Dogfooding | the agent memory | the Python package, as a user would use it | the Rust crates |
| Risk | many processes; shell parsing of the output; the allow-list is a safety boundary bent for a build | the PyO3 wheel must build in the docs job | most code to maintain |

**Recommendation at the time (not chosen): (b).** Superseded by the Rust tool: with mdBook, a
Rust preprocessor over `liquers-lib` (`templates`, `mem`) is the natural fit and starts the path of
§6. It is the smallest tool that does everything in §3. It uses the package
that the Python guide's examples need installed anyway. It is a real-world use of `liquers-py`, so
the documentation exercises it. Its one gap (section includes) is a small, separate change. Use (a)
only to prototype includes before (b) exists. Keep (c) in reserve if (b)'s build time or PyO3 in CI
proves a problem; `documentation-site-tool` is then revised to stage 1 only.

What (b) needs from Liquers, to be filed when Phase 2 confirms it:
- **A section include outside `mem`.** For example `ns-template/section-<heading>` over a
  Markdown state, or `mem` enabled in `liquers-py`.
- **Includes that report what they read**, so a chapter's dependencies on `specs/` are known
  without re-parsing the template. The template's recorded dependencies may already give this,
  once Phase 2 checks it.

Sources:
- [The state of Markdown book publishing (March 2026)](https://yeasy.blogspot.com/2026/03/the-state-of-markdown-book-publishing.html)
- [Material for MkDocs: status](https://pydevtools.com/handbook/reference/mkdocs-material/)
- [mdbook-epub](https://docs.rs/crate/mdbook-epub/0.5.4)
- [mdbook-typst](https://docs.rs/crate/mdbook-typst/latest)

## 5. Tool-neutral chapters

What a future Liquers tool would have to reimplement is whatever tool-specific syntax the chapters
contain. Measured that way:
- **Sphinx** is the hardest to leave: directives and roles (`{toctree}`, `{ref}`, `(label)=`) and a
  global label index carry the structure and every reference.
- **Quarto** is in between: Pandoc fenced divs, attributes, `@ref` cross-references, and
  `{{< include >}}` shortcodes, which also collide visually with Mustache.
- **mdBook** is the easiest: CommonMark with tables and footnotes, plain relative `.md` links
  (which map onto Liquers keys), and a plain list of chapters (`SUMMARY.md`). Its own
  `{{#include}}` looks like a Mustache section and is not used until Liquers templates read it
  (`TEMPLATE-HAS-NO-MDBOOK-INCLUDE-SYNTAX`); then it becomes the preferred include, since it works
  under plain mdBook too.

Rule for every chapter, enforced by `liquers-docs check`:
- CommonMark plus tables and footnotes only.
- Liquers templates for includes and generated content.
- Relative links for references.
- The markers of `guide-outlines.md` §4; multi-paragraph notes use a marker paragraph per note
  rather than a fenced div.
- Structure lives in `guide.yaml`; `SUMMARY.md` and `book.toml` are generated.
- No renderer syntax: no `{{#include}}`, `{{#playground}}`, `::: div`, `{.class}`, `@ref`,
  directives or roles.

## 6. The path to a Liquers documentation tool

Each step keeps every chapter unchanged:

1. **Now:** `liquers-docs build` expands the guides into a build tree; mdBook and Pandoc render it.
2. **mdBook preprocessor:** `liquers-docs` speaks mdBook's preprocessor protocol, so `mdbook build`
   runs Liquers itself, and `mdbook serve` previews with live template expansion.
3. **mdBook backend:** an additional output (`[output.liquers]`) writes what mdBook does not, for
   example search records for `ns-search` and a memory profile over the guides, so agents read them
   through `ns-mem`.
4. **liquers-doc:** Liquers renders the HTML site itself (Markdown to HTML as a command, layout as a
   template, site search as a query over the records of step 3), and mdBook becomes optional.

The original Python LiQuer was used to build documentation, so this is a tried use case.
