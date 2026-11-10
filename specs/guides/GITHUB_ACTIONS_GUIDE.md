---
title: GitHub Actions Guide
kind: guide
audience: internal
area: [build, docs]
reviewed: 2026-11-10
---
# GitHub Actions Guide

What each workflow in `.github/workflows/` does, when it runs, what it needs, and how to add one.
`scripts/docs_index.py --check` warns when a workflow file is not named in this guide, so a new
workflow is described here in the pull request that adds it.

## 1. The workflows

| Workflow | Runs on | Does | Permissions and secrets |
|---|---|---|---|
| `build-matrix.yml` | PRs and `main` pushes touching a crate, `Cargo.toml`, `.cargo/` or the script | `scripts/check-build-matrix.sh`: every feature configuration, library and test targets, and wasm32; a `rustdoc` job that denies broken intra-doc links on the clean crates (`API_DOCUMENTATION_GUIDE.md` §10) | read only; none |
| `docs-check.yml` | PRs and `main` pushes touching `specs/`, `docs/`, a crate's `src/`, `liquers-py/python/`, the index and API-doc scripts or the skill scripts | `docs_index.py --check`, `check_api_docs.py` (module docs against `API_DOCUMENTATION_GUIDE.md` §3, warnings only), and the unit tests of both scripts and of the `liquers-project` skill scripts | read only; none |
| `liquers-py-package.yml` | PRs touching `liquers-py` packaging or sources, the packaging scripts or either py workflow | builds the Linux x86_64 abi3 wheel as the release does, checks it, runs the installed-wheel tests; publishes nothing | read only; none |
| `liquers-py-release.yml` | `liquers-py-v*` tags; PRs changing the workflow; manual | builds, checks and tests the sdist and fifteen wheels; on a tag publishes to a GitHub release, TestPyPI, then PyPI after the `pypi` environment's approval | release: `contents: write`; publish: `id-token: write` (trusted publishing), environments `testpypi`, `pypi`; no secrets |
| `liquers-py-tests.yml` | PRs and `main` pushes touching a crate `liquers-py` builds from | `scripts/test-liquers-py.sh` on CPython 3.10, 3.14 and free-threaded 3.14t | read only; none |
| `liquers-web-tests.yml` | PRs and `main` pushes touching a crate `liquers-web` builds from, `docs/guides/web/` or `JAVASCRIPT_GUIDE.md` | the wasm32 conformance suites under Node, with and without `records`; then the JavaScript examples of `JAVASCRIPT_GUIDE.md` and of the Web user guide, against a `records` build | read only; none |
| `docs-site.yml` | PRs touching `docs/`, `specs/`, `liquers-docs/`, the three docs scripts or the workflow; `main` pushes also on every crate and `Cargo.toml` | checks and builds the user guides, every API reference (Rust crates, Python, JavaScript) and the browser module published at `pkg/liquers-web/`; deploys to GitHub Pages from `main` | build: read only; deploy: `pages: write`, `id-token: write`; no secrets |

All of them also run on `workflow_dispatch` (the *Run workflow* button). The Rust ones share a
pattern: `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2` (saved only from `main`
where the cache would otherwise be thrashed), and, where
the build is large, a *Free runner disk* step that removes preinstalled toolchains the job does
not use. `Cargo.lock` is not committed, so a job that needs an exact dependency version resolves
it first (`cargo generate-lockfile` in `liquers-web-tests.yml`).

## 2. `docs-site.yml`: the documentation site

The site is <https://orest-d.github.io/liquers/>: the landing page, the home page, the three user guides
(`docs/guides/`, see [`USER_DOCS_EDITING_GUIDE.md`](USER_DOCS_EDITING_GUIDE.md)) and the API
references. Only sources are committed; the site, API references included, exists as a build
output and a Pages deployment. Its layout:

| Path | Content | Made by |
|---|---|---|
| `index.html`, `favicon.ico`, `images/` | the landing page | copied from `docs/site/`, over the home book's `index.html` |
| `overview.html`, `guides/index.html` | the home page (every guide and API reference) and the guides index | mdBook, from `docs/index.md` and `docs/guides/index.md` |
| `guides/<rust,python,web>/` | the three guides, with search | mdBook |
| `downloads/` | EPUB, PDF and the offline HTML zip of each guide | Pandoc, mdBook |
| `api/rust/<crate>/` | every workspace crate's Rust API, one rustdoc tree; `liquers_web` is documented for wasm32 and copied in | `cargo doc` |
| `api/python/liquers.html` | the Python package | pdoc, `scripts/build-liquers-py-docs.sh` |
| `api/js/` | the JavaScript module of `liquers-web` (with `records`) | TypeDoc over the wasm-bindgen `.d.ts`, `scripts/build-liquers-web-docs.sh` |

The home page links each of them, and `liquers-docs check` fails when a workspace member of
`Cargo.toml` has no `api/rust/<crate>/index.html` link there, so a new crate cannot be left off.

Job `build`, on every trigger:

1. Installs the wasm32 target, Python and Node, a `wasm-bindgen` CLI of the version
   `cargo generate-lockfile` resolves, and pinned release binaries of `mdbook`, `pandoc` and
   `typst` (versions in the workflow's `env:`, with those of TypeDoc and TypeScript, which `npx`
   fetches).
2. `liquers-docs check` and `liquers-docs check --release`. The second fails a pull request that
   leaves `TBD:` or `CHECK:` in a `ready` chapter, which would otherwise fail only after merging.
3. `scripts/build-docs.sh`: a draft build on pull requests, a release build (`--release`) on
   `main`.
4. *Verify outputs* (`build-docs.sh --verify-only`): every guide has its online HTML with a search
   index, an offline zip without one, an EPUB and a PDF; every `api/...` page the home page
   links exists; and every page the landing page links to exists, its "coming soon" cards
   included.
5. Uploads the site: as the `docs-site` artifact on pull requests (download it from the run's
   summary page and open `index.html`), or as the Pages artifact on `main`.

Job `deploy`, on `main` only: `actions/deploy-pages` publishes the Pages artifact. It runs in the
`github-pages` environment and the `pages` concurrency group, so two pushes never deploy at once.

**One-time setup, by a repository owner:** *Settings → Pages → Build and deployment → Source:
GitHub Actions*. Until then `deploy` fails with GitHub's "Pages site not found" message; nothing
else is affected.

Running it locally: `bash scripts/build-docs.sh --skip-api` with the three tools installed, or
`cargo run -p liquers-docs -- check` with none. Without `--skip-api` it also needs Python 3, Node
(`npx`), the wasm32 target and `wasm-bindgen`; a missing one skips its reference with a notice.

To bump a tool, change its version in `env:` and check the *Tool versions* step's output on the
pull request; the asset names in the install step follow each project's release naming.

## 3. Adding a workflow

1. Name the file after what it checks or produces (`<crate>-tests.yml`, `<thing>.yml`).
2. Start from the closest existing workflow and keep its conventions: a comment at the top saying
   what it runs and where its design is, `paths:` filters so it runs only when its inputs change
   (and on the workflow file itself), `workflow_dispatch`, and `permissions: contents: read`
   unless it must write.
3. Put the logic in a script under `scripts/` that also runs locally, and call it from one step.
   CI then proves the same command a developer runs.
4. Describe it in §1 of this guide, in the same pull request.

## 4. Reserved: publishing packages

npm is not implemented yet; it gets a design and a section here when it is.

- **`liquers-py` to PyPI.** Implemented by `liquers-py-release.yml` (§1;
  `specs/design/liquers-py-package/`).
- **`liquers-web` to npm or a CDN.** Planned: the wasm package built by `wasm-bindgen`, published
  on a release tag; the npm token, if npm needs one, as an environment secret.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-10 | §1: `liquers-web-tests.yml` also runs the guides' JavaScript examples; `docs-site.yml` publishes the browser module. | writing the Rust and Web guides |
| 2026-11-09 | §2: the landing page (`docs/site/`) is the site's `index.html`; the home page moves to `overview.html`; *Verify outputs* checks the landing page's links. Maintaining the site as a whole: [`PROJECT_SITE_GUIDE.md`](PROJECT_SITE_GUIDE.md). | PR #266 |
| 2026-11-03 | §1: `build-matrix.yml`'s `rustdoc` job; `docs-check.yml` runs `check_api_docs.py` and triggers on source files. | `design/api-documentation-rules/` |
| 2026-11-03 | §1: `liquers-py-package.yml` and `liquers-py-release.yml`, merged from `main`; §4 PyPI points to them. | merge of `design/liquers-py-package` |
| 2026-11-02 | §1, §2: the site carries every API reference (all crates, Python, JavaScript); its layout, the new setup steps and the home-page link check. | `design/user-documentation-guides` (phase-4 addendum) |
| 2026-11-01 | Created: the five workflows, `docs-site.yml` and its Pages setup, adding a workflow, reserved publishing sections. | `design/user-documentation-guides` (phase-5) |
