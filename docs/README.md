# docs/

The sources of the public documentation site, <https://orest-d.github.io/liquers/>. Only sources
are committed; the site is built by CI (`.github/workflows/docs-site.yml`) and published to
GitHub Pages.

| Path | What |
|---|---|
| `site/` | the landing page, the site's `index.html`: plain HTML with its logo and favicon, copied to the site root as is. A link not yet published carries `data-soon` (a "coming soon" badge); remove it when the target is live |
| `index.md` | the home page, every guide and API reference, published as `overview.html` |
| `guides/index.md` | how the three guides relate |
| `guides/{rust,python,web}/` | the guides: `guide.yaml` (title, chapter order), the chapters, `HISTORY.md` |
| `guides/_shared/` | language-neutral fragments, included by chapters of all three guides |
| `theme/liquers.css` | the shared style of every book |

Chapters are Liquers templates expanded by `liquers-docs`, then rendered by mdBook (HTML) and
Pandoc (EPUB, PDF). How to write and maintain them: `specs/guides/USER_DOCS_EDITING_GUIDE.md`.
How to build them: `bash scripts/build-docs.sh` (or `cargo run -p liquers-docs -- check`).
How the site is deployed, and how to maintain and extend it and its landing page:
`specs/guides/PROJECT_SITE_GUIDE.md`.

This folder is not under `specs/`: it is written for users, and nothing in it links into `specs/`.
