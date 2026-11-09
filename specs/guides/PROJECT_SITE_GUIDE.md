---
title: Project Site Guide
kind: guide
audience: internal
area: [docs, build]
reviewed: 2026-11-09
---
# Project Site Guide

How to deploy, maintain and extend the project's public web site,
<https://orest-d.github.io/liquers/>: its landing page, the home page, the user guides and the API
references. This guide is the way in. It owns the landing page and how the parts fit together, and
it points to the guides that own the rest: chapters are
[`USER_DOCS_EDITING_GUIDE.md`](USER_DOCS_EDITING_GUIDE.md), module docs are
[`API_DOCUMENTATION_GUIDE.md`](API_DOCUMENTATION_GUIDE.md), and the workflow that builds and
deploys the site is [`GITHUB_ACTIONS_GUIDE.md`](GITHUB_ACTIONS_GUIDE.md) §2.

## 1. What the site is made of

Only sources are committed. The site exists as the output of `scripts/build-docs.sh` and as a
GitHub Pages deployment; nothing generated is ever written into `docs/`.

| Published path | What | Source | Made by |
|---|---|---|---|
| `index.html`, `favicon.ico`, `images/` | the landing page | `docs/site/` | copied as is |
| `overview.html` | the home page: every guide and every API reference, one line each | `docs/index.md` | mdBook (the home book) |
| `guides/index.html` | how the three guides relate | `docs/guides/index.md` | mdBook (the home book) |
| `guides/<rust,python,web>/` | the user guides, with search | `docs/guides/<guide>/` | `liquers-docs`, then mdBook |
| `downloads/` | EPUB, PDF and offline HTML of each guide | the same | Pandoc, Typst, mdBook |
| `api/rust/<crate>/` | the Rust API of every workspace crate | the crates' rustdoc | `cargo doc` |
| `api/python/liquers.html` | the Python API | `liquers-py` | pdoc, `scripts/build-liquers-py-docs.sh` |
| `api/js/` | the JavaScript API | `liquers-web` | TypeDoc, `scripts/build-liquers-web-docs.sh` |

Two layers of navigation, deliberately different:

- **The landing page** is for a first visit: what Liquers is in one sentence, an example query,
  and one card per guide and per API reference. It is hand-written HTML, short, and changes
  rarely.
- **The home page** (`overview.html`) is the complete map, in Markdown, one line per crate, kept
  complete by checks (§3.3). The landing page's Rust API card points into it, because a single
  card cannot list thirteen crates.

`docs/` is user-facing and must not link into `specs/` (`DOCS_STRUCTURE_GUIDE.md` §9.6); that
holds for the landing page too.

## 2. Deploying

### 2.1 Automatic deployment

Every push to `main` that touches `docs/`, `specs/`, `liquers-docs/`, a crate, `Cargo.toml` or the
build scripts runs `docs-site.yml`: it builds the site in release mode and deploys it to Pages.
There is no manual step and no `gh-pages` branch. A pull request runs the same build in draft mode
and uploads the site as the `docs-site` artifact of its run; download it and open `index.html` to
review the result before merging.

To redeploy without a change (after a failed deploy, or after the one-time setup below), run the
workflow by hand: *Actions → docs-site → Run workflow* on `main`.

### 2.2 One-time setup

A repository owner sets *Settings → Pages → Build and deployment → Source* to **GitHub Actions**.
Until then the `build` job succeeds and the `deploy` job fails within a second with GitHub's
"Pages site not found" (or "Get Pages site failed") error. Nothing else needs configuring: the
`github-pages` environment is created on the first deployment.

### 2.3 A failed deployment

| Symptom | Cause | Fix |
|---|---|---|
| `deploy` fails in about a second | Pages is not set to GitHub Actions | §2.2 |
| `deploy` refused: branch not allowed | the `github-pages` environment restricts deployment branches | allow `main` under *Settings → Environments → github-pages* |
| *Check the guides (release rules)* fails | a `ready` chapter still has `TBD:` or `CHECK:` | resolve the marker, or set the chapter back to `draft` (`USER_DOCS_EDITING_GUIDE.md` §4) |
| *Verify outputs* reports `missing: …` | a page the home page or the landing page links to was not built | build locally (§3.1) and fix the link or the build step |
| `liquers-docs check` reports a crate without an API link | a workspace member has no line in `docs/index.md` | add its `api/rust/<crate>/index.html` line (§4.3) |

### 2.4 The address

The site lives under `/liquers/` on `orest-d.github.io`. The address is written in three places,
which change together if it ever moves (a custom domain, a renamed repository):
`SITE_URL`, `SITE_PATH` and `REPOSITORY_URL` in `liquers-docs/src/build.rs` (mdBook's `site-url`
and the cross-guide links in EPUB and PDF), and the GitHub links in `docs/site/index.html`. A custom
domain also needs a `CNAME` file in `docs/site/`, which the build copies to the site root, and the
domain set under *Settings → Pages*.

## 3. Maintaining

### 3.1 Building and previewing locally

```bash
bash scripts/build-docs.sh --skip-api --skip-pdf   # the site in target/docs-site, in about a minute
bash scripts/build-docs.sh                         # everything, API references included
cargo run -p liquers-docs -- check                 # the guides' checks only; needs no tool
```

The build needs `mdbook` (the version pinned in `docs-site.yml`; a release binary from
<https://github.com/rust-lang/mdBook/releases> is enough), and for the downloads `pandoc` and
`typst`. A missing tool skips its step with a notice, except under `--release`. The landing page
needs no build: open `docs/site/index.html` in a browser directly. Open
`target/docs-site/index.html` to see the assembled site; links between the parts work from the
file system.

### 3.2 The landing page

`docs/site/index.html` is one self-contained file: inline CSS, no JavaScript, no web fonts, no
external libraries, so it loads instantly and has nothing to update. Its images are in
`docs/site/images/`; the logo and favicon come from the former `liquer` project's site.

- **Colours** are CSS variables on `:root`: the dark theme first, a light theme under
  `@media (prefers-color-scheme: light)`. `--accent` is the logo's green. Change a colour there,
  never inline, and check both themes.
- **Wording** follows [`../reference/VISION.md`](../reference/VISION.md). Keep it short: a
  tagline, the example query, three one-line points. Detail belongs in the guides.
- **The example query** must be a valid one. Check it with `liquers-validate` (`CLAUDE.md`
  §Validating queries). It is split into `<span class="seg">` path segments so it wraps only
  between them on a narrow screen; keep that structure when replacing it.
- **Layout** must work from 390 px wide up, with no horizontal scrolling. Check a change at phone
  width and in both colour schemes (browser developer tools are enough).

### 3.3 "Coming soon" cards

A guide or API card that is not published yet carries the attribute `data-soon`. CSS gives it a
"coming soon" badge and makes it unclickable. Its `href` is already the real path, so publishing
it is **deleting `data-soon` from that `<a>`** and nothing else. The build verifies every page the
landing page links to, cards with `data-soon` included (`build-docs.sh --verify`), so a card can be
turned on as soon as the target is ready, with no risk of a dead link.

Turn a card on when its target is worth reading, not merely when it exists: a guide when its first
chapters are `ready`, an API reference when its module docs meet `API_DOCUMENTATION_GUIDE.md`.

### 3.4 What else is kept in step

| When this changes | Update | Checked by |
|---|---|---|
| A crate is added to the workspace | its line in `docs/index.md` | `liquers-docs check` |
| A page the landing page links to moves | the `href` in `docs/site/index.html` | `build-docs.sh --verify` |
| The project's purpose or messaging | the tagline and points of the landing page | review against `VISION.md` |
| A tool version (mdBook, Pandoc, Typst, TypeDoc) | the workflow's `env:` | the PR's `docs-site` run |
| The site's layout | §1 of this guide and `GITHUB_ACTIONS_GUIDE.md` §2 | `docs_index.py --check` (History, `reviewed:`) |

The chapters and their review cycle are `USER_DOCS_EDITING_GUIDE.md` §7; module docs are
`API_DOCUMENTATION_GUIDE.md` §10.

## 4. Extending

### 4.1 A section or card on the landing page

Copy an existing `<section>` or `<a class="card">` and keep the structure: a card has a
`<span class="lang">` title with its two-letter badge and one `<p>` line. Add `data-soon` if the
target is not published. Before adding a section, ask whether it belongs in a guide instead; the
landing page stays one screen of reasons plus the documentation cards.

### 4.2 Another hand-written page or asset

Anything placed in `docs/site/` is copied to the site root unchanged: another HTML page, an image, a
`CNAME`, a `robots.txt`. A new HTML page starts with its own `<!doctype html>`, charset and viewport
meta, and reuses the landing page's `:root` variables. Avoid the names the build writes
(`overview.html`, `print.html`, `404.html`, and the folders `guides/`, `downloads/`, `api/`,
`css/`, `fonts/`): `docs/site/` is copied after mdBook, so a file there overwrites a generated
one of the same name.

### 4.3 A user guide, an API reference, a download

- **A chapter or a guide:** `USER_DOCS_EDITING_GUIDE.md`. A new guide is a folder in `docs/guides/`
  with a `guide.yaml`; `liquers-docs` finds it, and the build renders it and its downloads. Give it
  a line in `docs/index.md` and `docs/guides/index.md`, and a card on the landing page.
- **A crate's Rust API:** automatic once the crate is a workspace member; add its line to
  `docs/index.md`.
- **The Python or JavaScript API:** built by `scripts/build-liquers-py-docs.sh` and
  `scripts/build-liquers-web-docs.sh`. A new language integration adds a script of its own, a step
  in `build-docs.sh` writing under `api/<language>/`, its `expect` lines in the verify step, a line
  in `docs/index.md` and a card on the landing page.

### 4.4 Versioned documentation

The site has one version: the latest `main`. Publishing documentation per release would mean
building into `v<version>/` folders and keeping the older ones across deployments, which a Pages
artifact deployment does not do (each deployment replaces the whole site). That needs a design
before it is built: [`../issues/DOCS-SITE-HAS-NO-PER-RELEASE-VERSIONS.md`](../issues/DOCS-SITE-HAS-NO-PER-RELEASE-VERSIONS.md).

## History

| Date | Change | Source |
|---|---|---|
| 2026-11-09 | Created: the site's parts, deployment and its one-time setup, local builds, the landing page and its "coming soon" cards, extending the site. | landing page PR #266 |
