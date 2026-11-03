# Phase 5: Documentation - api-documentation-rules

## Completion Preconditions

- [x] Implementation is finished and validated — commit `a1ce5f1` and the follow-ups on
  `claude/funny-noether-o4vjv7`; the checks under *Validation*
- [x] All user comments are answered or incorporated — Phase 1 `proceed` (2026-11-01), Phase 2
  amended and pre-approved with `proceed all` (2026-11-03); Phase 5 approved with the merge
- [x] All review comments are answered or incorporated — each phase reviewed by the author
  against `review-checklist.md` and `validate_phase.py`
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical — same branch

## Implementation Summary

The request (GitHub #232) asked for module documentation rules, a language-agnostic guide with
Rust, Python and JavaScript sections, their place in the design process and skill, and a user
documentation folder. Items 1-3 are implemented here; item 4 was delivered by
`user-documentation-guides` while this design was at its Phase 2 gate, and Phase 2 was amended to
drop the `docs/` tree accordingly.

Built, as approved:

- `specs/guides/API_DOCUMENTATION_GUIDE.md`: the five parts of a module doc with their limits, the
  language terms, links, executed examples, a Rust, a Python and a JavaScript section, the checks,
  and the design phases that ask for it.
- `scripts/check_api_docs.py` with `scripts/test_check_api_docs.py` (19 tests): the Rust module
  walk (public when every `mod` on the path is `pub`), the Python docstrings, the JavaScript
  `@packageDocumentation` block; warnings for missing docs, headings, link counts and example
  length; an error for a GitHub `blob/main` link to a missing path. At HEAD it reports 730
  warnings over 315 modules and no error, the backlog of
  `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`.
- Worked examples: `liquers-core/src/commands.rs` (doctest registers a command and evaluates it),
  `liquers-py/python/liquers/__init__.py` (a `>>>` example, run by the new
  `pytest --doctest-modules` step of `scripts/test-liquers-py.sh`), `liquers-web/src/lib.rs` and a
  `@packageDocumentation` custom section in `typescript.rs` whose `@example` is the guide's first
  query. `liquers-docs/src/lib.rs`, already in this shape, is named as the shortest example.
- The `liquers-project` skill: the `- API docs:` line in the Phase 1 template and the compact
  template, module docs in the Phase 2, 4 and 5 templates, three checklist items, `auto-fix.md`
  rule 2, the phases table, and `validate_phase.py`'s date-gated rule (designs created from
  2026-11-03) with four tests.
- CI: `docs-check.yml` runs the checker and its tests and triggers on source files;
  `build-matrix.yml` gains a `rustdoc` job that denies broken intra-doc links on
  `liquers-template`, `-store`, `-mcp` and `-markdown`.

Changed from the approved design: the `commands.rs` doctest has ten visible lines rather than
eight, because `register_command!` needs `Context` in scope and a `CommandEnvironment` alias, and
`Environment::to_ref` must run inside the runtime (it spawns the asset manager's task); the design
chose to show these rather than hide them. Nothing was omitted.

## Documentation Delivered

### New Reference Documents
None: the rules are instructions for contributors, not a description of runtime behaviour.

### New Guide Documents
- `specs/guides/API_DOCUMENTATION_GUIDE.md` — the content rules, per language, and the checks.

### Existing Documents Reviewed or Updated
`affects_docs`: `guides/LANGUAGE-INTEGRATION_GUIDE.md`, `DOCS_STRUCTURE_GUIDE.md`.

- `DOCS_STRUCTURE_GUIDE.md`: §9.3 gains the module-doc obligation, §10 a row; History row dated
  2026-11-03 (the document carries no `reviewed:` front-matter).
- `guides/LANGUAGE-INTEGRATION_GUIDE.md`: §DOCS's generated-reference bullet and `DOCS04` name
  the guide; History row and `reviewed: 2026-11-03`.
- `guides/GITHUB_ACTIONS_GUIDE.md` (not in `affects_docs`; changed because the workflows changed):
  the two workflow rows; History row; `reviewed:` already 2026-11-03.
- `CLAUDE.md`: an *API docs* paragraph in Documentation. `liquers-py/README.md`: the test loop
  names the doctest step. `liquers-web/README.md`: the `DOCS` row names the module doc.

### Module Docs
Reviewed against the guide §3; `python3 scripts/check_api_docs.py` on the five files reports
`6 modules, 0 warnings, 0 errors` (`test_repository_worked_examples_are_clean`):
`liquers-core/src/commands.rs`, `liquers-py/python/liquers/__init__.py`,
`liquers-web/src/lib.rs`, `liquers-web/src/typescript.rs`, `liquers-docs/src/lib.rs`.

### User-Guide Chapters
`docs_index.py chapters-for` lists no chapter whose `sources:` name either `affects_docs`
document; nothing to review.

### Links and Capability Map
`specs/README.md`: a "When to use what" row for the guide; the capability line *Content rules
for API documentation* moved from designing to built, pointing at the guide.

## Issues Filed

- `WEB-DOCS-SCRIPT-RELATIVE-OUT-PATH-LANDS-IN-WORK-DIR` (S, tooling, eligible for automatic
  fixing): `build-liquers-web-docs.sh --out <relative>` writes inside its work directory and
  reports no `index.html`. Found verifying Step 5; spun off to a second child session (below). `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` was extended with the three
  `liquers-records` links found by the preflight and spun off (below). `WEB-API-REFERENCE-NOT-GENERATED`
  was found half stale (the reference builds; the per-export check is missing) and left for its
  next triage, as recorded in Phase 2's preflight.

## Important Learning

- The rustdoc tree the site publishes (`api/rust/`) makes cross-crate module links possible from
  a crate that does not depend on the target: a relative link from the page's own depth
  (`../../liquers_lib/image/index.html`). Rustdoc does not check such links; the site's verify
  step and a reader do.
- A `//!` example that shows the real entry point of `liquers_core::commands` needs the macro's
  two conventions (`Context` in scope, the `CommandEnvironment` alias) and a runtime around
  `to_ref`; hiding them with `# ` would hand a reader an example that does not compile when
  copied. The guide's "visible lines" rule is about what a reader needs, not what fits.
- `typescript_custom_section` strings are raw strings: a JSDoc block that contains `"#` (any
  `startsWith("# …")`) must use `r##"…"##`.
- wasm-bindgen emits custom sections in source order at the top of the `.d.ts`, so the
  `@packageDocumentation` constant has to be the first one in `typescript.rs` (its doc comment
  says so).

## Conformance and Remaining Work

Requested: items 1-5 of #232. Approved (Phase 1, amended Phase 2): items 1-3 here, item 4 by
`user-documentation-guides`, item 5 as `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES`.
Implemented: items 1-3 in full; AC-1 to AC-5 by this design, AC-6 by `user-documentation-guides`
(`docs/README.md`, `docs/index.md` with every crate's API link, check 18).

Remaining, each with an issue:
- `MODULE-DOCS-DO-NOT-FOLLOW-THE-API-DOC-RULES` — the 730 warnings; when it closes, the checker's
  warnings become errors (`--strict` in `docs-check.yml`).
- `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` — spun off to the child session
  `session_01Dx1JCYavM4zMRMrJXoe9fX`, branch `claude/rustdoc-intra-doc-links` (mechanism A). At
  the time of writing it is blocked on a Bash permission: it wants to port a `docs-site` venv fix
  from PR #245 into its branch so that CI can pass, which is outside the spin-off's scope and the
  owner's call. When it merges, `liquers-core` and `liquers-records` join the `rustdoc` job.
- `WEB-DOCS-SCRIPT-RELATIVE-OUT-PATH-LANDS-IN-WORK-DIR` — spun off to the child session
  `session_01Dx1JCXwrQASFHmQncNRcRx`, branch `claude/web-docs-relative-out` (mechanism A).

## Validation

Runs on 2026-11-03, on `claude/funny-noether-o4vjv7`:

| Check | Result |
|---|---|
| `python3 -m unittest scripts.test_check_api_docs` | 19 passed |
| `python3 -m unittest discover -s .claude/skills/liquers-project/scripts -p 'test_*.py'` | 12 passed |
| `python3 scripts/check_api_docs.py` | 315 modules, 730 warnings, 0 errors (exit 0) |
| `python3 scripts/check_api_docs.py` on the five worked-example files | 0 warnings, 0 errors |
| `validate_phase.py api-documentation-rules 1…5` | pass (size warnings on Phases 1 and 2) |
| `python3 scripts/docs_index.py --check` | 0 errors |
| `cargo test -p liquers-core --doc` | 24 passed, 7 ignored (the new `commands` doctest among the passed) |
| `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links" cargo doc --no-deps -p liquers-core` | 26 unresolved, none in `commands.rs` (unchanged; the spin-off's job) |
| the `rustdoc` job's command (`-template -store -mcp -markdown`) | exit 0 |
| `bash scripts/test-liquers-py.sh` | 411 passed, 23 skipped, 1 xpassed; the docstring doctest 1 passed; 38 Rust tests passed |
| `cargo check -p liquers-web --target wasm32-unknown-unknown` | exit 0 |
| `liquers-web/scripts/check-stubs.sh --build` | all stub and package checks passed; the generated `liquers_web.d.ts` opens with the `@packageDocumentation` block, after wasm-bindgen's two `/* … */` lines |
| `bash scripts/build-liquers-web-docs.sh` (TypeDoc 0.28.20) | the module page carries the package doc with its sections (`liquers-web/tests/e2e` needs `npm install` for the stubs check; see the issue below for the script's relative `--out`) |
