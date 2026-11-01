# Phase 1: High-Level Design — Documentation tool and site build

## Purpose

Build the user documentation site defined by `design/api-documentation-rules/` with one script:
the API docs come from the usual generators (rustdoc for every crate, the Python and JavaScript
integrations' own generators), and the homepage, guides, indexes and command reference are
rendered by **Liquers itself** — Mustache templates with query tags, evaluated by a documentation
tool. It serves `VISION.md`'s documentation and agent-usability goals, and makes the docs a worked,
continuously exercised example of the template system.

## Problem Example

Today there is nothing to run. `cargo doc` is run by hand; `scripts/` holds
`check-build-matrix.sh`, `docs_index.py` and `test-liquers-py.sh`; no command turns a folder of
Markdown templates into HTML pages (`pulldown-cmark` is only in the wasm `webui` feature).

Should be:

```bash
bash scripts/build-docs.sh                # → target/docs-site/index.html
```

which (1) runs `cargo doc --no-deps` for the workspace crates, and for `liquers-web` on
`wasm32-unknown-unknown`, plus the Python and JavaScript generators when their toolchains are
present, placing each under `api/<language>/<package>/`; (2) runs the documentation tool over
`docs/`; (3) checks every relative link in the result. A homepage source in `docs/index.md`:

```markdown
# Liquers documentation

{{#$ ns-docs/guides}}
- [{{title}}]({{href}}) — for {{audiences}}
{{/}}

Command library: {{#$ ns-docs/namespaces}}[{{.}}](reference/commands/{{.}}.html) {{/}}
```

rendered with a page pipeline such as
`-R/guides/python-developer/index.md/-/ns-template/expand/ns-docs/page` (Markdown to HTML inside
the `templates/page.html` layout and the site navigation), and the command reference page of a
namespace from the registry, e.g. `commands_doc-img`. (Queries validated with `liquers-validate`;
`ns-docs/page`, `ns-docs/guides` and `ns-docs/namespaces` are proposed commands, validated with `--command`.)

## Scope and Acceptance Criteria

- **AC-1** One script builds the site
  WHEN `bash scripts/build-docs.sh` runs on a checkout
  THEN `target/docs-site/` holds `index.html`, every page of `docs/` as HTML, rustdoc for every
  workspace crate under `api/rust/`, and an `api/index.html` linking each package of each language
- **AC-2** Pages are Liquers templates
  WHEN a page under `docs/` contains query tags (`{{$ …}}`, `{{#$ …}}`) or partials
  THEN the tool evaluates them through a Liquers environment over the `docs/` folder and writes
  the rendered page as HTML in the site layout, with the site navigation
- **AC-3** Indexes are generated, not hand-written
  WHEN a guide folder with a `guide.yaml` is added under `docs/guides/`
  THEN the next build lists it on the homepage and on `guides/index.html`, grouped by its audiences,
  with no other file edited
- **AC-4** The command reference is generated
  WHEN the site is built
  THEN `reference/commands/` has a page per command namespace, built from the registered commands
- **AC-5** Missing generators are reported, not fatal
  WHEN the Python or JavaScript toolchain is absent
  THEN the script skips that language with a notice on stderr, the API landing page says the
  reference was not built, and the rest of the site builds
- **AC-6** Broken links fail the build
  WHEN a page or a module doc links to a site path that does not exist
  THEN the build fails naming the page and the target
- **AC-7** Errors point at the source
  WHEN a template tag is invalid or a query fails
  THEN the build fails naming the source file and position (as `ns-template/validate` does)

Non-goals: the rules and the `docs/` structure (`api-documentation-rules`); choosing the Python and
JavaScript generators (`py-api-reference`, `WEB-API-REFERENCE-NOT-GENERATED`); publishing (GitHub
Pages) and versioned sites; search inside the site; a live server (a later `liquers-axum` mount
could serve the same pages).

## Core Interactions

- Store: a read-only file store over `docs/`, and a write target for the site.
- Commands: `ns-template` (`expand`, `render`, `validate`), `commands_doc`, new `ns-docs` commands.
- Value types: text and HTML results; no new `ExtValue` variant expected.
- Assets: pages are evaluated as queries, so partials and query results are dependencies.

## Crate Placement

Open (question 1). The tool needs `liquers-lib` with `templates` and a native Markdown renderer, so
it sits above `liquers-lib`; nothing below it changes except an optional dependency.

## Documentation Intent

- Reference: none new; `TEMPLATE_LANGUAGE.md` unchanged unless commands are added to
  `ns-template`.
- Guide: extend `specs/guides/API_DOCUMENTATION_GUIDE.md` (from the companion design) with
  "Building the docs", or a short `docs/README.md` section — the latter if contributors only.
- API docs: the tool's crate or module follows the API documentation guide.
- Documents to update: `CLAUDE.md` (building the docs), `TEMPLATE_GUIDE.md` (the site as a worked
  example), `specs/README.md`, `specs/command_registry.yaml` if `ns-docs` commands are registered in
  `liquers-lib`.

## Open Questions

1. **Where the tool lives (open design).** Recommended: a new crate `liquers-docs` (binary plus
   its `ns-docs` commands) depending on `liquers-lib`, like `liquers-agent`; it keeps a Markdown
   renderer and site logic out of the library's default build. Alternative: a `docs` feature and
   `[[bin]]` in `liquers-lib`, which makes the commands available to every environment.
2. **How a page's query is chosen (open design).** Recommended: rules in `docs/site.yaml`
   (`*.md` → `ns-template/expand/ns-docs/page`, `*.yaml` → none), applied by walking the source
   folder, with a page's front matter able to override. Alternative: every page an explicit
   recipe in a `recipes.yaml`, built by a generic "materialize every recipe into a store" step —
   more Liquers-native and reusable, but every new page then needs a recipe line.
3. **Markdown renderer (proposed resolution).** `pulldown-cmark`, already a `liquers-lib`
   dependency under `webui`, made available natively to the tool.
4. **Link checking (proposed resolution).** The tool checks relative links in every written HTML
   file, rustdoc output included, so the cross-crate links that `api-documentation-rules`
   question 3 proposes are verified.
5. **CI (open design).** Recommended: a `docs-site.yml` job that builds the Rust part and the
   pages on each PR touching `docs/` or doc comments, uploading the site as an artifact; Python and
   JavaScript references join when their generators are chosen.

## Design Dependencies

- `api-documentation-rules` — **requires**: the `docs/` structure, `site.yaml` and the output
  location (`target/docs-site/`) come from it.
- `py-api-reference`, `WEB-API-REFERENCE-NOT-GENERATED` — **overlaps**: the script calls the
  generators they choose; until then AC-5 applies.
- `liquers-lib/src/template/folder.rs` — `FolderNode` already lets a template read a folder
  (`-R-dir/guides`) as a map of children; Phase 2 decides whether `ns-docs/guides` is needed or a
  folder read suffices (it would need each `guide.yaml` to read as a map, and an ordering).

## References

- GitHub #233; owner's request of 2026-11-01.
- `specs/guides/TEMPLATE_GUIDE.md`, `specs/reference/TEMPLATE_LANGUAGE.md` §4.
- `liquers-agent/` — the existing binary-over-`liquers-lib` pattern.
