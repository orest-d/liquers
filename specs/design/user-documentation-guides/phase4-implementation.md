# Phase 4: Implementation Plan — User guides, their formats, publishing and CI

## Overview

The build order is: the crate first, because everything else is checked by it; then the skeleton
it checks; then the script and CI that run both; then the index rules; then the documents.
Prerequisites: none blocking (Phase 2 preflight). The `docs/` root is created here. When
`api-documentation-rules` lands later, it adds its API landing pages beside the guides and reuses
check 18 from Step 5.

Pre-approved after Phase 3 on 2026-11-01 (`proceed all`). Decision log at the pre-implementation
stop: no open decision. One owner action remains, which does not block implementation: enabling
GitHub Pages with Source "GitHub Actions" before the first `main` deploy.

## Progress

- [ ] Step 1: `liquers-docs` crate — configuration, markers, neutral check, include command
- [ ] Step 2: `liquers-docs` — environment, expansion, build tree, CLI
- [ ] Step 3: `docs/` skeleton — three guides, shared fragments, home book, theme
- [ ] Step 4: `scripts/build-docs.sh`, `docs-site.yml`, `CODEOWNERS`
- [ ] Step 5: `docs_index.py` — chapters, `HISTORY.md` rows, check 18, `chapters-for`, Actions guide check
- [ ] Step 6: `USER_DOCS_EDITING_GUIDE.md`, `GITHUB_ACTIONS_GUIDE.md`
- [ ] Step 7: contract and pointer updates — `DOCS_STRUCTURE_GUIDE.md`, `CLAUDE.md`, `LANGUAGE-INTEGRATION_GUIDE.md`, skill, `specs/README.md`
- [ ] Step 8: a full local build where the tools can be installed; regenerate indexes

## Implementation Steps

### Step 1: crate, configuration and text passes

- Files / symbols: `Cargo.toml` (`members`); `liquers-docs/Cargo.toml`;
  `liquers-docs/src/{lib,guide,chapter,markers,neutral,include}.rs`.
  - `GuideConfig`, `ChapterMeta`, `ChapterStatus`, `Marker`, `BuildMode`, `Finding`, `Severity`.
  - `apply_markers` and `check_neutral`.
  - `include_section(text, heading) -> Result<String, Error>` and the `include` command.
- Change: as in Phase 2 Interfaces.
- Depends on: none.
- Proof: `cargo test -p liquers-docs --lib` passes tests 4-8 and 10.
- Rollback: drop the member line and the folder.

### Step 2: environment, expansion, build tree, CLI

- Files / symbols:
  - `liquers-docs/src/{env,build,main}.rs`: `build_environment`, `expand_chapter`, `build`,
    `check`, `report`, and `clap` subcommands.
  - `liquers-docs/tests/{expand,build}.rs`.
- Change: the environment follows `liquers-agent/src/lib.rs`. Its mounts are `docs/` and
  `specs/`, read-only, from an in-code `StoreRouterConfig`. The build tree and `book.toml` are as
  in Phase 2.
- Depends on: Step 1.
- Proof: `cargo test -p liquers-docs` passes tests 9 and 11. If the `..` partial fails, switch the
  shared fragments to absolute keys and record that in the Learning Log.
- Rollback: as Step 1.

### Step 3: `docs/` skeleton

- Files: `docs/README.md`, `docs/index.md`, `docs/guides/index.md`, `docs/guides/_shared/*.md`,
  `docs/theme/liquers.css`, and for each of `docs/guides/{rust,python,web}/`: `guide.yaml`,
  `index.md`, the 19 chapter and appendix files of `guide-outlines.md` §2, and `HISTORY.md`.
- Change:
  - Every chapter has `status: outline`, `PURPOSE:`, `TBD:` per section, `SOURCE:` lines and
    `sources:`.
  - Chapter 1's hello-world is copied from the test that runs it, and the test is named.
  - Appendix B is a template over the registry.
- Depends on: Step 2, which checks it.
- Proof: `cargo run -p liquers-docs -- check` reports no errors. `skeleton.rs::guides_are_parallel`
  (test 3) passes.
- Rollback: delete `docs/guides/`.

### Step 4: build script, workflow, CODEOWNERS

- Files: `scripts/build-docs.sh`, `.github/workflows/docs-site.yml`, `.github/CODEOWNERS`.
- Change:
  - The script and the workflow follow Phase 2's Integration Points, with the `specs/**` trigger
    and `check --release` on PRs.
  - `mdbook`, `pandoc` and `typst` are pinned release binaries.
  - A `verify outputs` step implements CI check 14.
  - The `deploy` job runs on `main` only.
- Depends on: Steps 2-3.
- Proof: `bash -n scripts/build-docs.sh`; a local run (Step 8); the PR's own `docs-site` run.
- Rollback: delete the workflow; nothing else depends on it.

### Step 5: documentation index rules

- Files / symbols: `scripts/docs_index.py`:
  - `collect` reads `docs/guides/*/[0-9a-z]*.md` as kind `chapter`;
  - `check` warns on `reviewed:` without a `HISTORY.md` row, adds check 18 (no link from
    `docs/` into `specs/`) and adds the Actions guide coverage warning;
  - a `chapters-for` subcommand;
  - the 92-day rule covers chapters with `reviewed:`.
- `scripts/test_docs_index.py` gets `ChapterTests` and `ActionsGuideTests`.
- Depends on: Step 3 (real chapters to index).
- Proof: `python3 -m unittest -v scripts/test_docs_index.py`; `python3 scripts/docs_index.py
  --check` reports 0 errors.
- Rollback: revert the script; the generated files are regenerated.

### Step 6: the two guides

- Files: `specs/guides/USER_DOCS_EDITING_GUIDE.md`, `specs/guides/GITHUB_ACTIONS_GUIDE.md`.
- Change: their content as in Phase 2's Documentation Architecture, each with front matter
  (`kind: guide`, `audience: internal`, `reviewed: 2026-11-01`) and a `## History`.
- Depends on: Steps 3-5 (they describe them).
- Proof: `docs_index.py --check` passes; `ActionsGuideTests` passes against the real workflows.
- Rollback: delete them.

### Step 7: contract and pointer updates

- Files:
  - `specs/DOCS_STRUCTURE_GUIDE.md`: §2, §3 (`docs` covers `docs/guides/`), §4.2 `chapter`, §7.2
    checks, §9.6, §10, History.
  - `CLAUDE.md`: Project Structure, Documentation, Building.
  - `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §DOCS, with a History row.
  - `.claude/skills/liquers-project/references/phase5-documentation.md`: chapters for
    `affects_docs` via `docs_index.py chapters-for`.
  - `.claude/skills/liquers-project/references/review-checklist.md`.
  - `specs/README.md`.
- Depends on: Steps 5-6.
- Proof: `docs_index.py --check`; `python3 -m unittest discover -s .claude/skills/liquers-project/scripts`.
- Rollback: revert the files.

### Step 8: full build and indexes

- Install `mdbook`, `pandoc` and `typst` where the environment allows. Run `bash
  scripts/build-docs.sh`, check the outputs against check 14, and regenerate `specs/index.csv` /
  `index.md`.
- If a tool cannot be installed here, the PR's CI run is the proof, and the PR says so.

## Testing Plan

- After Steps 1-2: `cargo test -p liquers-docs`.
- After Steps 3-7: `cargo run -p liquers-docs -- check`; `python3 -m unittest -v
  scripts/test_docs_index.py`; `python3 scripts/docs_index.py --check`; the skill scripts' tests.
- Final: Step 8, then the PR's `docs-site` and `docs` workflow runs.
- `scripts/check-build-matrix.sh` is not needed: no `cfg(feature)` or optional dependency of an
  existing crate changes. `liquers-docs` uses fixed `liquers-lib` features.

## Rollback Plan

Every step is additive.
- Reverting the PR removes the crate, `docs/`, the workflow and the document changes together.
- If only Steps 1-4 land, the guides build without index integration.
- If Step 5 lands without Step 6, the Actions guide check warns and does not fail.

## Documentation Updates

- New: the two guides (Step 6), `docs/README.md`, and the `liquers-docs` module docs.
- Updated: Step 7's list.
- Regenerated: `specs/index.csv` and `specs/index.md`.
- History rows and `reviewed:` bumps: `DOCS_STRUCTURE_GUIDE.md` and
  `LANGUAGE-INTEGRATION_GUIDE.md`.
- `specs/command_registry.yaml` is unchanged: the `ns-docs` commands are registered by
  `liquers-docs`, not by `liquers-lib`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
