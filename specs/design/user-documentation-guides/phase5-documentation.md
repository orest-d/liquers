# Phase 5: Documentation - user-documentation-guides

## Completion Preconditions

- [x] Implementation is finished and validated (locally; the PR's `docs-site` run is its CI proof)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none on the implementation yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #234)

## Implementation Summary

Liquers now has public user documentation that is separate from `specs/`, built from sources and
published by CI:

- **Three parallel guides** in `docs/guides/{rust,python,web}/`: *Liquers for Rust*, *for Python*
  and *for the Web*, each a `guide.yaml`, a preface, chapters 1-15, appendices A-D and a
  `HISTORY.md`. Every chapter is `outline`: a `PURPOSE:`, a `TBD:` per section with its content
  proposal and `SOURCE:`, and `sources:` front matter naming the `specs/` documents it is written
  from. Chapter 1 carries each language's hello-world, copied from the test that runs it.
  Language-neutral parts (queries, assets, templates, errors) are `_shared/` fragments. Appendix B
  is generated from `specs/command_registry.yaml`; appendix C includes the glossary from
  `PROJECT_OVERVIEW.md`. A home page and a guides index sit in `docs/`.
- **`liquers-docs`**, a new workspace crate (not a default member): builds a Liquers environment
  with `docs/` and `specs/` mounted read-only, expands each chapter with `ns-template/expand`,
  applies the editorial-marker rules (draft and release), refuses renderer-specific syntax, checks
  links, and writes the mdBook books and one Pandoc file per guide. Two commands, `ns-docs/include`
  (a `specs/` section by heading, with its subsections) and `ns-docs/command_table`. CLI: `build`,
  `check [--release]`, `report`. 26 tests, including one over the real `docs/guides/`.
- **`scripts/build-docs.sh`**: mdBook twice (with search online, without it for the offline zip),
  Pandoc for EPUB and PDF (Typst, or LaTeX with `--latex`), `cargo doc`, assembly, `--verify`.
- **`.github/workflows/docs-site.yml`**: check and draft build on pull requests (the site as an
  artifact), release build and GitHub Pages deployment on `main`. `.github/CODEOWNERS` asks the
  owner to review `docs/guides/`.
- **Maintenance**: `docs_index.py` indexes chapters (kind `chapter`), checks their status, sources,
  review dates and `HISTORY.md` rows, forbids links from `docs/` into `specs/` (check 18), warns on
  an undescribed workflow, and lists the chapters of a spec (`chapters-for`) for Phase 5 reviews.

**Conformance.** It matches the request (three guides on the old LiQuer model, a marker
convention, an editing guide, an Actions guide with PyPI/npm reserved, only sources on `main`,
HTML with and without search, EPUB, PDF via Typst or LaTeX) and the approved design (mdBook +
Pandoc, a Rust preprocessor, tool-neutral chapters). Added: `ns-docs/command_table` and the check
for markers arriving through fragments. Changed from Phase 2: the chapter folder is passed as the
template's `cwd`, and `build_environment` is synchronous ([implementation notes](phase4-implementation.md#implementation-notes)).
Not done here, by scope: writing chapter text (editorial work); Python and JavaScript API
references (existing issues).

## Documentation Delivered

### New Reference Documents
None. The contract for chapters is part of `DOCS_STRUCTURE_GUIDE.md` (§4.2, §7.2, §9.6), and the
build is described in the two guides below and in `liquers-docs`' module documentation.

### New Guide Documents
- [`guides/USER_DOCS_EDITING_GUIDE.md`](../../guides/USER_DOCS_EDITING_GUIDE.md): layout, chapter
  format and statuses, style, markers, includes and their rules, the tool-neutral rule,
  maintenance, building and previewing.
- [`guides/GITHUB_ACTIONS_GUIDE.md`](../../guides/GITHUB_ACTIONS_GUIDE.md): every workflow,
  `docs-site.yml` and the Pages setup, adding a workflow, reserved PyPI and npm sections.
- `docs/README.md`: what the `docs/` folder holds, for anyone opening it.

### Existing Documents Reviewed or Updated
Authoritative `affects_docs`: `DOCS_STRUCTURE_GUIDE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`.

- `DOCS_STRUCTURE_GUIDE.md`: §2 and §3 (`docs/` exists), §4.2 kind `chapter`, §7.2 checks 18-20,
  §9.6 rewritten (guides are written from `specs/` and include from it; chapters are reviewed),
  §10, History row.
- `guides/LANGUAGE-INTEGRATION_GUIDE.md` §DOCS: a language's users get a guide in
  `docs/guides/<language>/`; the contract reference and topic guide stay in `specs/`. History row;
  `reviewed:` already 2026-11-01.
- Candidates sharing the `docs`/`build` areas, discarded after reading: `VISION.md`,
  `POSITIONING.md`, `CONFORMANCE_TERMS.md`, `api/API_DOCS_GAP_ANALYSIS.md`,
  `AGENT_MEMORY_MIGRATION_GUIDE.md`, `UNITTEST_GUIDE.md`, `autonomous_issue_fixing.md`,
  `autonomous_bulk_design.md`: none describes user documentation, its build or CI.
- Also updated: `CLAUDE.md` (structure, the user-guides rule, the test and build commands), the
  `liquers-project` skill (`phase5-documentation.md` and `review-checklist.md`: review the
  chapters `chapters-for` lists).

### User-Guide Chapters
`chapters-for` lists none for this design's `affects_docs`; all 60 chapters are new outlines.

### Links and Capability Map
`specs/README.md`: two "When to use what" rows (editing a chapter; CI and publishing) and two
capability lines under *Build and repository* (the user guides; API documentation rules).

## Issues Filed

- `QUERY-SYNTAX-HAS-NO-INCLUDABLE-SECTION`: appendix A cannot include the query syntax, because
  the `PROJECT_OVERVIEW.md` section holds relative links.
- `USER-GUIDE-EXAMPLES-ARE-NOT-EXECUTED`: chapter examples are not run (deferred in Phase 3).
- `DRAFT-MARKERS-LOOK-ALIKE`: draft HTML styles every marker kind the same way.
- `GUIDE-SECTION-ANCHORS-DIFFER-BETWEEN-RENDERERS`: section links between chapters break in EPUB
  and PDF.
- `LIQUERS-DOCS-DUPLICATES-MARKDOWN-PARSING`: `liquers-markdown` landed on `main` in parallel;
  `liquers-docs` should read Markdown through it.
- `LIQUERS-DOCS-HAS-NO-LIVE-PREVIEW` (feature): the mdBook preprocessor step toward a Liquers-based
  documentation tool.

Closed: `USER-GUIDES-LIVE-IN-SPECS`, `USER-DOCUMENTATION-CANNOT-BE-GENERATED`.

## Important Learning

- **An include and a memory read are different units.** `ns-mem/read`'s section is one heading's
  own text; an include needs the heading's subtree. `ns-docs/include` exists for that.
- **A clean `liquers-validate` result does not prove an argument decodes as intended**: `~_` passed
  validation but decodes to `-`; a space is `~.`. Only the expansion test caught it.
- **An ad-hoc query has no working directory**, so a template's relative partials need the folder
  passed explicitly (`expand`'s `cwd` argument).
- **Owner action before the first deploy:** Settings → Pages → Source: GitHub Actions.

## Conformance and Remaining Work

Requested, approved and implemented scope agree, apart from the additions and changes above. The
remaining work is editorial (writing chapters) and the five issues filed; nothing is left
partially implemented.

## Validation

- `cargo test -p liquers-docs`: 18 unit, 7 integration, 1 doctest, all pass.
- `cargo run -p liquers-docs -- check`: 0 errors, 0 warnings over the real `docs/guides/`.
- `bash scripts/build-docs.sh --skip-api --verify` with mdbook 0.5.4, Pandoc 3.1.3, Typst 0.15.1:
  every guide's HTML (with search), offline zip (without), EPUB and PDF; `verify: ok`.
- `python3 -m unittest scripts.test_docs_index`: 64 tests pass (new `ChapterTests`,
  `ActionsGuideTests`); `python3 scripts/docs_index.py --check`: 0 errors.
- `liquers-project` skill script tests pass; `validate_phase.py user-documentation-guides 5`.
