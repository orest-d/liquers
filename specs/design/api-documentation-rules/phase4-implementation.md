# Phase 4: Implementation Plan — api-documentation-rules

## Overview

The guide first, because every other step cites it; then the checker, so the worked examples can
be checked as they are written; then the three worked examples; then the skill, CI and the spec
updates. The rustdoc job ships with the four clean crates. Prerequisite for widening it:
`RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED`, fixed in a spin-off (Step 9), not in this PR.

## Progress
- [x] Step 1: Write `specs/guides/API_DOCUMENTATION_GUIDE.md` — a1ce5f1
- [x] Step 2: Write `scripts/check_api_docs.py` and its tests — a1ce5f1
- [x] Step 3: Document `liquers_core::commands` — a1ce5f1
- [x] Step 4: Document the `liquers` Python package — a1ce5f1
- [x] Step 5: Document `liquers_web` and its `@packageDocumentation` — a1ce5f1, 1a16426
- [x] Step 6: Put the rules into the `liquers-project` skill — a1ce5f1
- [x] Step 7: CI: the checker step and the rustdoc job — a1ce5f1
- [x] Step 8: Update the specs, `CLAUDE.md` and the READMEs — a1ce5f1
- [x] Step 9: Spin off the intra-doc link fix — child session `session_01Dx1JCYavM4zMRMrJXoe9fX`, branch `claude/rustdoc-intra-doc-links` (its PR is its proof; status in Phase 5)

## Implementation Steps

### Step 1: Write `specs/guides/API_DOCUMENTATION_GUIDE.md`
- Files / symbols: `specs/guides/API_DOCUMENTATION_GUIDE.md` (new; front-matter `kind: guide`,
  `audience: internal`, `area: [docs]`, `reviewed:`); `specs/README.md` "When to use what" row.
- Change: the sections of Phase 2 §Integration Points; the module-doc shape of Phase 2
  §Interfaces, with `liquers-docs/src/lib.rs` as the shown example.
- Depends on: none.
- Proof: `python3 scripts/docs_index.py --check` (index regenerated, no dead link).
- Rollback: delete the file and the row.

### Step 2: Write `scripts/check_api_docs.py` and its tests
- Files / symbols: `scripts/check_api_docs.py` (`rust_modules`, `python_modules`,
  `javascript_module`, `check_doc`, `main`); `scripts/test_check_api_docs.py`.
- Change: the command line, sources, warnings and errors of Phase 2 §Interfaces; text and JSON
  output; `--strict`. Fixture crates are written into a temporary directory by the tests.
- Depends on: Step 1 (the rules it encodes).
- Proof: `python3 -m unittest -v scripts.test_check_api_docs` (all Phase 3 unit tests except
  `test_repository_worked_examples_are_clean`, which passes after Step 5);
  `python3 scripts/check_api_docs.py` exits 0 with warnings.
- Rollback: delete both files.

### Step 3: Document `liquers_core::commands`
- Files / symbols: `liquers-core/src/commands.rs` (`//!` block before the `#![allow]` lines are
  kept: inner attributes come first, then the doc).
- Change: Phase 3 Example 1.
- Depends on: Step 2 (checked with it).
- Proof: `cargo test -p liquers-core --doc commands`; `python3 scripts/check_api_docs.py
  liquers-core/src/commands.rs` reports nothing; `RUSTDOCFLAGS="-D rustdoc::broken_intra_doc_links"
  cargo doc --no-deps -p liquers-core` reports no new unresolved link (26 at HEAD stay).
- Rollback: revert the file.

### Step 4: Document the `liquers` Python package
- Files / symbols: `liquers-py/python/liquers/__init__.py` docstring;
  `scripts/test-liquers-py.sh` (a `pytest --doctest-modules` line after the suite);
  `liquers-py/README.md` (the test loop paragraph).
- Change: Phase 3 Example 2.
- Depends on: Step 2.
- Proof: `bash scripts/test-liquers-py.sh` (needs `maturin`; in CI `liquers-py-tests.yml`);
  `python3 scripts/check_api_docs.py liquers-py/python/liquers/__init__.py` reports nothing.
- Rollback: revert the three files.

### Step 5: Document `liquers_web` and its `@packageDocumentation`
- Files / symbols: `liquers-web/src/lib.rs` `//!`; `liquers-web/src/typescript.rs`
  (new `const LIQUERS_PACKAGE_DOC: &str` under `#[wasm_bindgen(typescript_custom_section)]`).
- Change: Phase 3 Example 3.
- Depends on: Step 2.
- Proof: `cargo check -p liquers-web --target wasm32-unknown-unknown`;
  `./liquers-web/scripts/check-stubs.sh --build` (the `.d.ts` still parses; needs `wasm-bindgen`);
  `python3 scripts/check_api_docs.py liquers-web` reports nothing; now
  `test_repository_worked_examples_are_clean` passes.
- Rollback: revert the two files.

### Step 6: Put the rules into the `liquers-project` skill
- Files / symbols: `.claude/skills/liquers-project/references/{phase1,phase2,phase4}-template.md`,
  `phase5-documentation.md`, `compact-design-template.md`, `review-checklist.md`, `auto-fix.md`;
  `SKILL.md` phases table; `scripts/validate_phase.py` (`API_DOCS_LINE = "2026-11-03"`,
  `api_docs_line_problems`); `scripts/test_validate_phase.py` `ApiDocsLineTests`.
- Change: Phase 2 §Integration Points, skill bullet. The Phase 1 rule: an error when
  `created:` ≥ the gate date and `## Documentation Intent` has no `- API docs:` line.
- Depends on: Step 1 (the checklist cites the guide).
- Proof: `python3 -m unittest discover -v -s .claude/skills/liquers-project/scripts -p 'test_*.py'`;
  `validate_phase.py api-documentation-rules 1` still passes (this design carries the line).
- Rollback: revert the skill directory.

### Step 7: CI: the checker step and the rustdoc job
- Files / symbols: `.github/workflows/docs-check.yml` (step "API documentation sections",
  paths `liquers-*/src/**`, `liquers-py/python/**`, `scripts/check_api_docs.py`,
  `scripts/test_check_api_docs.py`); `.github/workflows/build-matrix.yml` (job `rustdoc`).
- Change: Phase 2 §Integration Points, CI bullet; `specs/guides/GITHUB_ACTIONS_GUIDE.md` gets the
  job in its workflow table.
- Depends on: Step 2.
- Proof: the job's command locally, exit 0; `python3 scripts/docs_index.py --check` (check 20,
  workflow coverage); the PR's `docs` and `build-matrix` runs green.
- Rollback: remove the step and the job.

### Step 8: Update the specs, `CLAUDE.md` and the READMEs
- Files / symbols: `specs/DOCS_STRUCTURE_GUIDE.md` §9.3, §10, History, `reviewed:`;
  `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §DOCS (`DOCS04`), History, `reviewed:`;
  `CLAUDE.md` Documentation section; `liquers-web/README.md` `DOCS` row; `specs/README.md`
  capability line.
- Change: Phase 2 §Documentation Architecture.
- Depends on: Steps 1-7 (they describe what landed).
- Proof: `python3 scripts/docs_index.py --check`; the index regenerated.
- Rollback: revert the documents.

### Step 9: Spin off the intra-doc link fix
- Files / symbols: none here. A child session fixes `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED`
  on its own branch from `main` with the spin-off prompt (`references/spin-off.md`, mechanism A).
- Change: none in this PR. When it merges, `liquers-core` and `liquers-records` join the rustdoc
  job (a one-line follow-up, recorded in the issue's resolution).
- Depends on: none; can run in parallel with Steps 1-8.
- Proof: the child session's PR; its status is reported in Phase 5.
- Rollback: not applicable.

## Testing Plan

Steps 2 and 6 run their unit tests as they land. Step 3 runs the doctest; Step 5 the wasm32
check. Before Phase 5: the four commands of Phase 3 §Test Plan, `cargo test -p liquers-core --lib
--tests --doc` (the doctest sits beside the crate's own tests), `python3 scripts/docs_index.py
--check`, and the two CI runs on the PR. No `cfg(feature)` or dependency changes, so the feature
matrix is not run.

## Rollback Plan

Every step is a file set with no runtime effect; `git revert` of the PR restores HEAD. With only
some steps landed: the guide and checker without worked examples leave the checker's
`test_repository_worked_examples_are_clean` failing, so Steps 2-5 land together; the skill rule
without the guide would cite a missing document, so Step 6 follows Step 1.

## Documentation Updates

Created: `guides/API_DOCUMENTATION_GUIDE.md`, `scripts/check_api_docs.py` (self-documenting
`--help`). Updated with History rows and `reviewed:` bumps: `DOCS_STRUCTURE_GUIDE.md`,
`guides/LANGUAGE-INTEGRATION_GUIDE.md`, `guides/GITHUB_ACTIONS_GUIDE.md`. Updated without
front-matter: `CLAUDE.md`, `liquers-py/README.md`, `liquers-web/README.md`, `specs/README.md`.
Regenerated: `specs/index.csv`, `specs/index.md`. `specs/command_registry.yaml` is untouched.

## Phase 5 Entry Criteria
- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
