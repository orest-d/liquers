---
title: GitHub Actions Guide
kind: guide
audience: internal
area: [build, docs]
reviewed: 2026-11-01
---
# GitHub Actions Guide

What each workflow in `.github/workflows/` does, when it runs, what it needs, and how to add one.
`scripts/docs_index.py --check` warns when a workflow file is not named in this guide, so a new
workflow is described here in the pull request that adds it.

## 1. The workflows

| Workflow | Runs on | Does | Permissions and secrets |
|---|---|---|---|
| `build-matrix.yml` | PRs and `main` pushes touching a crate, `Cargo.toml`, `.cargo/` or the script | `scripts/check-build-matrix.sh`: every feature configuration, library and test targets, and wasm32 | read only; none |
| `docs-check.yml` | PRs and `main` pushes touching `specs/`, `docs/`, the index scripts or the skill scripts | `docs_index.py --check`, the unit tests of `docs_index.py` and of the `liquers-project` skill scripts | read only; none |
| `liquers-py-tests.yml` | PRs and `main` pushes touching a crate `liquers-py` builds from | `scripts/test-liquers-py.sh` on CPython 3.10, 3.14 and free-threaded 3.14t | read only; none |
| `liquers-web-tests.yml` | PRs and `main` pushes touching a crate `liquers-web` builds from | the wasm32 conformance suites under Node, with and without `records` | read only; none |
| `docs-site.yml` | PRs touching `docs/`, `specs/`, `liquers-docs/`, the build script or the workflow; `main` pushes also on `liquers-core/` and `liquers-lib/` | checks and builds the user guides and the API reference; deploys to GitHub Pages from `main` | build: read only; deploy: `pages: write`, `id-token: write`; no secrets |

All of them also run on `workflow_dispatch` (the *Run workflow* button). The Rust ones share a
pattern: `dtolnay/rust-toolchain@stable`, `Swatinem/rust-cache@v2` (saved only from `main`
where the cache would otherwise be thrashed), system OpenSSL headers (`libssl-dev`), and, where
the build is large, a *Free runner disk* step that removes preinstalled toolchains the job does
not use. `Cargo.lock` is not committed, so a job that needs an exact dependency version resolves
it first (`cargo generate-lockfile` in `liquers-web-tests.yml`).

## 2. `docs-site.yml`: the documentation site

The site is <https://orest-d.github.io/liquers/>: the home page, the three user guides
(`docs/guides/`, see [`USER_DOCS_EDITING_GUIDE.md`](USER_DOCS_EDITING_GUIDE.md)) and the Rust API
reference. Only sources are committed; the site exists as a build output and a Pages deployment.

Job `build`, on every trigger:

1. Installs pinned release binaries of `mdbook`, `pandoc` and `typst` (versions in the
   workflow's `env:`).
2. `liquers-docs check` and `liquers-docs check --release`. The second fails a pull request that
   leaves `TBD:` or `CHECK:` in a `ready` chapter, which would otherwise fail only after merging.
3. `scripts/build-docs.sh`: a draft build on pull requests, a release build (`--release`) on
   `main`.
4. *Verify outputs* (`build-docs.sh --verify-only`): every guide has its online HTML with a search
   index, an offline zip without one, an EPUB and a PDF; the API reference has
   `liquers_core/index.html`.
5. Uploads the site: as the `docs-site` artifact on pull requests (download it from the run's
   summary page and open `index.html`), or as the Pages artifact on `main`.

Job `deploy`, on `main` only: `actions/deploy-pages` publishes the Pages artifact. It runs in the
`github-pages` environment and the `pages` concurrency group, so two pushes never deploy at once.

**One-time setup, by a repository owner:** *Settings → Pages → Build and deployment → Source:
GitHub Actions*. Until then `deploy` fails with GitHub's "Pages site not found" message; nothing
else is affected.

Running it locally: `bash scripts/build-docs.sh --skip-api` with the three tools installed, or
`cargo run -p liquers-docs -- check` with none.

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

Not implemented yet; each gets a design and a section here when it is.

- **`liquers-py` to PyPI.** Planned: trusted publishing (OpenID Connect, `id-token: write`, no
  API token stored), wheels built by `maturin` for each supported interpreter, triggered by a
  release tag.
- **`liquers-web` to npm or a CDN.** Planned: the wasm package built by `wasm-bindgen`, published
  on a release tag; the npm token, if npm needs one, as an environment secret.

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-01 | Created: the five workflows, `docs-site.yml` and its Pages setup, adding a workflow, reserved publishing sections. | `design/user-documentation-guides` |
