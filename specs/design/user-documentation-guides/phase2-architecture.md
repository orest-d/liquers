# Phase 2: Solution & Architecture — User guides, their formats, publishing and CI

## Overview

The pipeline, from source to the published site:

1. **Expand.** A small Rust crate, `liquers-docs`, builds a Liquers environment with read-only
   mounts of `docs/` and `specs/`. It expands every chapter of the three guides with
   `ns-template/expand`, which resolves `_shared/` partials, `specs/` includes and generated tables.
2. **Check.** It applies the marker rules, the tool-neutral rule and the link rules.
3. **Write a build tree.** The tree holds an mdBook source folder per guide, with a generated
   `SUMMARY.md` and `book.toml`, plus one Pandoc input file per guide.
4. **Render.** `scripts/build-docs.sh` runs `mdbook build` twice (search on and search off),
   `pandoc` for EPUB and PDF, and `cargo doc` for the Rust API reference.
5. **Assemble.** It puts everything into one site folder.

`docs-site.yml` runs the check and a draft build on every PR, uploading the folder as an artifact.
On `main` it runs a release build and deploys to GitHub Pages through Actions.

**Assumptions from the Phase 1 gate** (`proceed`, 2026-11-01):
- Q1: a build tree, not mdBook's preprocessor protocol.
- Q2: Typst is the default PDF engine, with LaTeX as an option.
- Q3: anyone may edit through a PR; agents edit only when asked or within a design.
- Q4: search covers one guide at a time.

Rejected:
- **Pandoc for the HTML too.** No book navigation or search.
- **Committing the built site to a `gh-pages` branch.** The owner asked that the repository hold
  only sources, and the Actions deployment needs no branch.
- **One mdBook book for all three guides.** Parallel chapter numbers would collide, and the EPUB
  and PDF are per guide anyway.

## Known-Issue Preflight

| Issue / design | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `documentation-site-tool` (`USER-DOCUMENTATION-CANNOT-BE-GENERATED`) | Phase 2 in review | P2 | This design now covers its build script, homepage, API placement and CI. Its own page renderer is dropped | — | no | **Decision at this gate:** merge it into this design (question 1) |
| `api-documentation-rules` | Phase 2 in review | P2 | Supplies `docs/`, its link rules and `check_api_docs.py`. Its `docs/guides/` layout and `getting-started/` pages are replaced by this design | no | no | Amended at its Phase 3, as noted in its `DESIGN.md` |
| `TEMPLATE-HAS-NO-MDBOOK-INCLUDE-SYNTAX` | draft | P3 | Chapters use partials and query tags until it exists | no | no | None |
| `PY-API-REFERENCE-NOT-GENERATED`, `WEB-API-REFERENCE-NOT-GENERATED` | draft | P2/P3 | `api/python/` and `api/javascript/` are empty | no | no | The home book says "not built yet" |

## Interfaces

Crate `liquers-docs` (binary and library). It depends on `liquers-core`, `liquers-store`,
`liquers-lib` (`default-features = false`, `features = ["templates", "mem"]`), `serde`,
`serde_yaml`, `clap` and `tokio`. It is a workspace member and not a default member.

```rust
// guide.rs — docs/guides/<guide>/guide.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuideConfig {
    pub title: String,
    pub language: String,               // rust | python | web
    pub chapters: Vec<String>,          // file names, in order; "a-…" names are appendices
    #[serde(default)] pub authors: Vec<String>,
}
// chapter.rs — a chapter's front matter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterMeta { pub title: String, pub status: ChapterStatus,
    #[serde(default)] pub sources: Vec<String>, pub reviewed: Option<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChapterStatus { Outline, Draft, Review, Ready }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker { Tbd, Purpose, Source, Editor, Check }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode { Draft, Release }
pub struct Finding { pub file: String, pub line: usize, pub severity: Severity, pub message: String }
pub enum Severity { Warning, Error }

pub async fn build_environment(repo_root: &Path) -> Result<EnvRef<DocsEnvironment>, Error>;
pub async fn expand_chapter(env: &EnvRef<DocsEnvironment>, key: &Key) -> Result<String, Error>;
pub fn apply_markers(text: &str, status: ChapterStatus, mode: BuildMode) -> (String, Vec<Finding>);
pub fn check_neutral(text: &str) -> Vec<Finding>;   // forbidden renderer syntax (tooling.md §5)
pub async fn build(repo_root: &Path, out: &Path, mode: BuildMode) -> Result<Vec<Finding>, Error>;
```

- `expand_chapter` evaluates `-R/docs/guides/<guide>/<file>/-/ns-template/expand`. It is async
  because evaluation is.
- `apply_markers` and `check_neutral` are synchronous text passes over a chapter's lines, with no
  regex dependency.
- Release mode replaces a non-`ready` chapter with its title and "In preparation".

CLI (stdout carries only the summary or the requested report):

```text
liquers-docs build  [--root .] [--out target/docs-build] [--release]
liquers-docs check  [--root .] [--format text|json]     # every finding; exit 1 on an error
liquers-docs report [--root .] [--format text|json]     # markers, statuses, sources per chapter
```

The build tree:
- `target/docs-build/<guide>/book.toml` and `src/`, with `SUMMARY.md` generated from `guide.yaml`
  (appendices after a separator).
- `target/docs-build/<guide>.md`: the chapters concatenated in order, with a Pandoc YAML title
  block.
- `target/docs-build/home/`: the home book, from `docs/index.md` and `docs/guides/index.md`.

Generated `book.toml` (one per guide):

```toml
[book]
title = "Liquers for Python"
src = "src"
language = "en"
[output.html]
site-url = "/liquers/guides/python/"
git-repository-url = "https://github.com/orest-d/liquers"
additional-css = ["../../theme/liquers.css"]   # shared by all books; copied in from docs/theme/
[output.html.search]
enable = true          # MDBOOK_OUTPUT__HTML__SEARCH__ENABLE=false for the offline build
```

## Integration Points

- **New `liquers-docs/`:** `Cargo.toml`, `src/{main,lib,guide,chapter,markers,build}.rs`, and
  `tests/`. The environment follows `liquers-agent/src/lib.rs` `build_environment`, with mounts
  `docs` → `./docs` and `specs` → `./specs`, both `read_only`.
- **Workspace `Cargo.toml`:** add `liquers-docs` to `members` only.
- **`scripts/build-docs.sh [--release] [--out DIR] [--skip-api] [--skip-pdf]`**:
  1. `cargo run -p liquers-docs -- build`.
  2. For each guide, `mdbook build`, once more with search off, the second copy zipped.
  3. `pandoc <guide>.md -o <guide>.epub` and `--pdf-engine=typst -o <guide>.pdf`; `--latex` swaps
     the PDF engine.
  4. `cargo doc --no-deps --workspace --exclude liquers-py --exclude liquers-web`.
  5. Assemble the site: `/` (home), `/guides/<guide>/`, `/downloads/` (EPUB, PDF, offline zip),
     `/api/rust/`.
  6. A missing `mdbook`, `pandoc` or `typst` skips its step with a notice, except under
     `--release`.
- **`docs/`:**
  - `index.md` (home page).
  - `guides/index.md`.
  - `guides/_shared/` (fragments).
  - `guides/{rust,python,web}/` with `guide.yaml`, the 20 chapter files of `guide-outlines.md` §2
    in `status: outline`, and `HISTORY.md`.
  - `theme/liquers.css`.
  - `README.md`.
- **`.github/workflows/docs-site.yml`:**
  - **Pull requests** touching `docs/**`, `liquers-docs/**`, `scripts/build-docs.sh` or the
    workflow:
    1. Install Rust, then `mdbook` and `pandoc` (pinned release binaries), then `typst` (pinned
       release).
    2. `liquers-docs check`.
    3. `build-docs.sh`.
    4. `actions/upload-artifact`.
  - **Push to `main`:** the same with `--release`, then `actions/upload-pages-artifact` and
    `actions/deploy-pages`. Permissions `pages: write`, `id-token: write`; environment
    `github-pages`; concurrency group `pages`.
- **`.github/CODEOWNERS`:** `/docs/guides/ @orest-d`.
- **`scripts/docs_index.py`:**
  - indexes `docs/guides/*/*.md` chapters as kind `chapter` (front matter as above);
  - checks that a chapter with `reviewed:` has a row with that date in its guide's `HISTORY.md`;
  - puts `reviewed` chapters in the 92-day check;
  - check 18 (no link into `specs/`) covers `docs/`.
  - Tests go in `test_docs_index.py`.
- **`liquers-project` skill:** `phase5-documentation.md` lists the chapters whose `sources`
  intersect `affects_docs` as review candidates. `review-checklist.md` Phase 5 gets the same.

## Error Handling

| Failure | Result | Constructor |
|---|---|---|
| `guide.yaml` or front matter does not parse | Error naming the file | `liquers_core::validate::registry::from_json_or_yaml` (`Error::from_error`) |
| A chapter listed in `guide.yaml` is missing | Error naming the guide and file | `Error::key_not_found` |
| A template tag or include fails (missing partial, missing `specs/` section) | Error with chapter and position; strict mode | the `ns-template` error |
| A marker in a `ready` chapter in release mode; renderer syntax; a link into `specs/`; a relative link outside the build tree | `Finding` with `Severity::Error`; `build` and `check` exit 1 | — |
| A chapter not in `guide.yaml`; `reviewed:` with no `HISTORY.md` row | `Finding`, warning | — |
| An output write fails | Error naming the path | `Error::from_error(ErrorType::General, e)` |

## Relevant Commands

None new. Used: `ns-template/expand` for chapters, `ns-mem/read` for section includes
(`{{$ -R/specs/guides/PYTHON_GUIDE.md/-/ns-mem/read-full-<heading>}}`), and `commands_doc` /
the committed registry for appendix B (decided in Phase 3).

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `guides/USER_DOCS_EDITING_GUIDE.md` | guide | internal | docs | new. Covers: style (voice, person, tense, length, headings, code blocks); chapter template and statuses; markers; cross-guide references; examples and how they run; includes (when: wording that must stay identical, such as a syntax table, an option list or a signature; never narrative; a section by heading or a whole document for appendices; included sections must be self-contained and link-free); maintenance (`reviewed`, `sources`, `HISTORY.md`, Phase 5 trigger); the tool-neutral rule |
| `guides/GITHUB_ACTIONS_GUIDE.md` | guide | internal | build | new: every workflow (`build-matrix`, `docs-check`, `liquers-py-tests`, `liquers-web-tests`, `docs-site`) with triggers, jobs, permissions, secrets and how to add one. Reserved sections: publishing `liquers-py` to PyPI (trusted publishing) and `liquers-web` to npm / a CDN |
| `DOCS_STRUCTURE_GUIDE.md` | contract | internal | docs | §2 `docs/`; §4.2 kind `chapter`; §7.2 the new checks; §9.6 rewritten (`specs/guides/` is the basis; nothing moves); History |
| `guides/LANGUAGE-INTEGRATION_GUIDE.md` | guide | internal | docs | §DOCS: an integration's users get a guide in `docs/guides/<language>/`; History |
| `CLAUDE.md` | — | — | docs | structure (`docs/`, `liquers-docs/`), building the docs, the editing guide pointer |
| `liquers-docs/src/lib.rs` | API doc | user | docs | module doc per `API_DOCUMENTATION_GUIDE.md` |

Proposed `affects_docs`: `DOCS_STRUCTURE_GUIDE.md`, `guides/LANGUAGE-INTEGRATION_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | ~10 in the new crate; ~70 skeleton files under `docs/` (3 × 20 chapters, shared fragments, manifests); the script; 1 workflow; CODEOWNERS; `docs_index.py` and its tests; 4 specs documents; 2 new guides; 2 skill files |
| Crates and workflows affected | New `liquers-docs`; workspace `Cargo.toml`; new `docs-site.yml` |
| Existing tests likely to change | `test_docs_index.py` (new cases); none in Rust |
| New validation | Unit tests for markers, the neutral check, `SUMMARY.md` generation and expansion over a fixture tree in a memory store; `liquers-docs check` and a full build in CI |
| Compatibility / data / concurrency / performance / security | Mounts are read-only; chapters use `{{$ }}`, never `{{$$ }}`. The CI build compiles `liquers-lib` with two features (minutes) and `cargo doc` of the workspace with default features, which is the costliest step; it is measured in Phase 3 and cached with `Swatinem/rust-cache` |
| Recovery | Revert the workflow to stop deploying; the crate is not a default member |
| Certainty and open questions | Medium-high. To verify in Phase 3: relative partial keys with `..` (`{{> ../_shared/x.md}}`) resolve through `Key::to_absolute`, otherwise absolute keys are used; what `ns-mem/read` returns for a section (heading included or not); Pandoc's handling of mdBook-relative links between chapters in the concatenated file. **Needs the owner:** enabling Pages with Source "GitHub Actions" (Settings → Pages) before the first deploy |

## Questions at this gate

1. **Merge `documentation-site-tool` into this design (open design).** Recommended: yes. Its build
   script, homepage, API placement and CI are all here now, and its own Markdown-to-HTML and
   layout are dropped in favour of mdBook. It becomes `superseded`, and its issue
   `USER-DOCUMENTATION-CANNOT-BE-GENERATED` becomes a second source of this design
   (`merged: 2026-11-01`).
2. **Appendix B, the command library (proposed resolution).** Generated per namespace from the
   committed `specs/command_registry.yaml` (as `documentation-site-tool` decided), so the docs
   build needs no polars, egui or image features.
