# Phase 1: High-Level Design — API documentation rules and the user documentation folder

## Purpose

Give every module's API documentation — rustdoc, and its docstring and JSDoc equivalents in the
Python and JavaScript integrations — a defined content (purpose, main components, where it is
used, related concepts, examples), make writing it part of the design process and the
`liquers-project` skill, and give user documentation a top-level home. It serves `VISION.md`'s
agent-usability and integration goals: a reader, human or coding agent, can start from any module
and find its entry point, its neighbours and a working example.

## Problem Example

**API docs today.** `liquers-core/src/commands.rs` defines `CommandRegistry`, `CommandExecutor`
and the command call path, and has no `//!` doc at all; neither have `command_metadata.rs`,
`error.rs`, `metadata.rs`, `state.rs` or `value.rs` (85 of 276 source files under
`liquers-*/src/` at `15e06a4`). Opening `liquers_core::commands` in rustdoc shows an alphabetical
item list. Nothing says that `register_command!` is the usual entry point, that commands are
registered in `liquers-lib`'s `commands.rs`, `image/`, `records/` and `ui/`, or how a command
differs from a recipe. Where a doc exists (`store.rs`, `assets.rs`) it explains the module well
but not where it is used or which example to read.

**Should read** (the shape the guide prescribes; content illustrative):

```rust
//! Command registry and execution: how a query action becomes a function call.
//!
//! # Main components
//! - [`CommandRegistry`] — the commands an environment can run; filled by `register_command!`.
//! - [`CommandExecutor`] — calls a registered command with its resolved arguments.
//!
//! # Where it is used
//! Every command library registers through it: `liquers-lib` images (`image/`), records
//! (`records/`) and UI (`ui/`) are three typical ones; also every unit test that builds an
//! environment. The interpreter calls the executor for each `Action` step (`interpreter.rs`).
//!
//! # Related concepts
//! [Recipes](crate::recipes) name a query to run for a key; a command is one step of that
//! query. [`command_metadata`](crate::command_metadata) describes a command without running it.
//!
//! # Examples
//! ```
//! # (3-10 lines, executed as a doctest)
//! ```
//! - [`liquers-core/tests/async_hellow_world.rs`](…) — registration through evaluation, end to end.
```

**User documentation today:** none. `docs/` is reserved (`DOCS_STRUCTURE_GUIDE.md` §9.6);
user-audience guides (`PYTHON_GUIDE.md`, `JAVASCRIPT_GUIDE.md`) sit in the internal `specs/`.
**Should exist** — sources committed, the built site not (question 4):

```
docs/
  README.md            contributor note: what lives here, how it is built (not a site page)
  site.yaml            manifest: title, sections, guides, API packages per language
  index.md             homepage (Mustache with query tags), rendered to index.html
  getting-started/     one short page per language: install, first query
  guides/
    index.md           list of guides, grouped by audience (generated from guide.yaml files)
    <guide>/           one folder per guide: rust-developer, python-developer,
      guide.yaml       javascript-developer, agent-memory, data-science, frontend, backend…
      index.md         (title, audiences, languages, order)
      <chapter>.md
  concepts/            language-neutral explanations the guides share: queries, keys and
                       stores, assets, commands, values
  reference/           user-facing reference: query syntax, configuration documents, the
                       command library (generated from the registry)
  api/
    index.md           every package in every language
    rust/  python/  javascript/   an index.md each; generated docs placed beside it at build
  templates/           page layout and partials (not pages)
  assets/              CSS, images
```

Built site: the same tree as HTML, plus `api/rust/<crate>/`, `api/python/liquers/`,
`api/javascript/liquers-web/` and `reference/commands/<namespace>.html`. Future-proofing rests
on four rules: top-level folders are **kinds** of document (getting started, guides, concepts,
reference, API), never audiences, so a new audience is a `guide.yaml` value rather than a new
folder; a language is a path segment only under `api/` and `getting-started/`; every guide is a
folder with `index.md`, so it can grow chapters without its URL changing; every link is relative,
so the site can later be served under a version prefix (`/0.4/`, `/latest/`) unchanged.

## Scope and Acceptance Criteria

- **AC-1** The guide defines a module doc
  WHEN a contributor opens `specs/guides/API_DOCUMENTATION_GUIDE.md`
  THEN it states the five parts of a module doc (purpose, main components, where used, related
  concepts, examples) with their limits — main components are entry points, not an inventory;
  3, at most 5, linked uses plus a sentence on the rest, diverse when use is widespread; related
  concepts with the difference and a link; one 3-10 line executed example, up to 5 linked longer
  examples with a one-line description, and where to find more — in language-neutral terms
- **AC-2** Languages are covered
  WHEN the guide names a language object (module, struct / enum / trait versus class, protocol,
  interface, function)
  THEN it gives the Rust, Python and JavaScript term, and its Rust, Python and JavaScript sections
  say where each part is written (`//!`, module and class docstrings, JSDoc carried into the
  `.d.ts`), how links and executed examples work there, and what cannot be linked
- **AC-3** The rules are applied once per language
  WHEN the guide lands
  THEN `liquers_core::commands`, the `liquers` Python package and `liquers-web`'s entry module
  follow it, and their examples run (`cargo test --doc`, the Python and JavaScript example loops)
- **AC-4** The design process asks for it
  WHEN a `liquers-project` design is written
  THEN Phase 1's documentation intent names the modules whose API docs change, Phase 2 plans
  them, Phase 5 records their review against the guide, the review checklist asks for each, and
  `validate_phase.py` refuses a Phase 1 without the API docs line
- **AC-5** A check reports gaps
  WHEN the documentation check runs
  THEN it lists every public module without a module doc or without the guide's sections, and
  fails on a broken intra-doc link
- **AC-6** The user documentation folder exists
  WHEN `docs/` is opened
  THEN it has the structure above with a homepage source, an API landing page per language and a
  `README.md` explaining the layout, and `DOCS_STRUCTURE_GUIDE.md` describes `docs/` as existing,
  with `docs_index.py --check` refusing a link from `docs/` into `specs/`

Non-goals: building or publishing the site (`design/documentation-site-tool/`); choosing the
Python or JavaScript generator (`design/py-api-reference/`, `WEB-API-REFERENCE-NOT-GENERATED`);
rewriting every existing module doc (`MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`); writing the
future user guides beyond any moved by question 6.

## Core Interactions

Documentation only: `specs/` rules and the skill, three module docs (Rust, Python, JavaScript),
a check script, and the new `docs/` sources. No runtime behaviour changes.

## Crate Placement

No new code in a crate beyond doc comments in `liquers-core`, `liquers-py` and `liquers-web`. The
check is a Python script in `scripts/`, beside `docs_index.py`, run by `docs-check.yml`.

## Documentation Intent

- Reference: none — the rules are how-to for contributors.
- Guide: new `specs/guides/API_DOCUMENTATION_GUIDE.md` (internal: it addresses people writing
  docs, not reading them).
- API docs: `liquers-core/src/commands.rs`, the `liquers` Python package docstring, the
  `liquers-web` entry module — the worked examples of AC-3.
- Other documents: `docs/README.md`, `docs/index.md`, `docs/api/**/index.md`, `docs/site.yaml`.
- Documents to update: `DOCS_STRUCTURE_GUIDE.md` §2, §9.6, §10; `CLAUDE.md` (structure and a
  pointer to the guide); `LANGUAGE-INTEGRATION_GUIDE.md` §DOCS (`DOCS04` points to the guide; the
  user guide's home is `docs/guides/`); the `liquers-project` skill (`SKILL.md`, phase templates,
  `review-checklist.md`, `auto-fix.md`, `validate_phase.py`); `specs/README.md`.

## Open Questions

1. **Fixed headings (open design).** Recommended: yes — the purpose is the first paragraph
   (rustdoc's summary line), then `# Main components`, `# Where it is used`, `# Related concepts`,
   `# Examples`, in that order, in every language, so readers can scan and the check can find
   them. A small or private module needs only the purpose; the guide gives the threshold (public,
   or more than one public item).
2. **Links to examples and tests outside the crate (open design).** Rustdoc can link only to
   items. Recommended: a GitHub link pinned to `main`
   (`https://github.com/orest-d/liquers/blob/main/<path>`), which the check verifies by mapping
   back to the local path. Alternatives: the repository path in a code span (no link, always
   valid), or a link into the built site.
3. **Uses in crates that are not dependencies (open design).** `liquers-core` cannot intra-doc
   link to `liquers-lib`, which uses it. Recommended: a relative link into the combined rustdoc
   output (`../../liquers_lib/image/index.html`), valid because the site builds every crate
   together and checked by the site build's link check; otherwise the full path in a code span.
4. **Generated output: committed or not (open design).** Recommended: `docs/` holds sources only;
   the build writes the whole site, API docs included, to `target/docs-site/`, never committed;
   publishing (GitHub Pages) is a later choice. Committing generated rustdoc into `docs/api/`
   would put tens of megabytes of churn in every PR.
5. **Check severity (proposed resolution).** A missing module doc or section is a warning listed
   on every run; a broken intra-doc link fails. Modules a design touches must conform at its
   Phase 5. The warning becomes a failure when `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` is
   closed.
6. **Move the existing user guides now (open design).** Recommended: move `PYTHON_GUIDE.md` and
   `JAVASCRIPT_GUIDE.md` into `docs/guides/python-developer/` and `javascript-developer/` in this
   design ("move, never fork", §9.6), rewriting their links into `specs/`, so the homepage has
   real content; the other user-audience guides follow one by one. Alternative: ship the
   structure empty and move later.

## Design Dependencies

- `documentation-site-tool` — **required-by**: builds the site from the `docs/` sources defined
  here.
- `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` — **required-by**: the rewrite of existing docs.
- `py-api-reference`, `WEB-API-REFERENCE-NOT-GENERATED` — **overlaps**: they choose the Python
  and JavaScript generators; this design fixes where their output goes (`api/<language>/`).
- `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` — **requires** before AC-5's link check can fail the
  build (26 broken links today); `S`, likely eligible for automatic fixing.
- `NO-SOURCE-CODE-AND-API-DOC-NAVIGATION` — weak overlap only (code items as records); not merged.

## References

- GitHub #232; owner's request of 2026-11-01.
- `liquers-core/src/store/pipeline.rs` (PR #231): the nearest existing module doc to the target.
- `DOCS_STRUCTURE_GUIDE.md` §9.6; `LANGUAGE-INTEGRATION_GUIDE.md` §DOCS;
  `reference/api/API_DOCS_GAP_ANALYSIS.md`.
