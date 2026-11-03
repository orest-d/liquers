# Phase 2: Solution & Architecture — Documentation tool and site build

## Overview

A new crate, `liquers-docs`, provides a library, a binary `liquers-docs` with `build` and `check`
subcommands, and the `ns-docs` commands. It is a workspace member outside `default-members`, like
`liquers-agent`. The tool builds a Liquers environment over `docs/`. Each source page is evaluated
as a query (`-R/<page>.md/-/ns-template/expand-~X~ns-docs/site~E`), so its query tags and partials
run in Liquers. The resulting Markdown is converted to HTML, wrapped in the `templates/page.html`
Mustache layout and written to the output folder. `scripts/build-docs.sh` runs rustdoc and the
language generators first, then the tool, and the tool's link check runs last.

Rejected:
- **Pages as recipes** in a `recipes.yaml`, materialized by a generic step (Phase 1 Q2,
  alternative). Every page would need a recipe line, and AC-3 asks that adding a guide edit
  nothing else.
- **A `[[bin]]` in `liquers-lib`** (Q1, alternative). It would add `pulldown-cmark` and site code
  to every native build of the library.
- **A static-site generator** (mdBook, Zola). The owner asked for Liquers queries and Mustache,
  and the site doubles as a worked example of them.

**Assumptions taken at the Phase 1 gate** (`proceed` on 2026-11-01, questions not answered): Q1 new
crate; Q2 rules in `site.yaml`; Q3 `pulldown-cmark`; Q4 the tool checks links; Q5 a CI job
uploading the site, triggered by its own paths and weekly.

**Changed from Phase 1's illustration:** the proposed `ns-docs/page` is split into the tool's
layout step and `ns-docs/md_to_html`; `ns-docs/guides` and `ns-docs/namespaces` become fields of
`ns-docs/site`, passed as the page's data, so the homepage reads `{{#guides}}…{{/guides}}` rather
than a query tag per list. The acceptance criteria are unchanged.

**Decided here:** the command reference is rendered from the committed
`specs/command_registry.yaml`, not from the live registry. With the live registry, the tool would
have to link `liquers-lib` with every optional feature (polars, egui, image) to document every
command. The committed file holds them all and is checked for freshness by `registry_export`. The
tool therefore depends on `liquers-lib` with `default-features = false, features = ["templates"]`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `api-documentation-rules` (design) | Phase 2 | — | Defines `docs/`, `site.yaml`'s purpose and `target/docs-site/` | yes | yes | Its `docs/` step lands first. This design owns `site.yaml`'s schema |
| `PY-API-REFERENCE-NOT-GENERATED` | needs-decision | P2 | No Python generator yet | no | no | The script calls `scripts/build-liquers-py-docs.sh` when it exists; otherwise AC-5 |
| `WEB-API-REFERENCE-NOT-GENERATED` | draft | P3 | No TypeDoc step yet | no | no | Same, with `liquers-web/scripts/build-api-docs.sh` |
| `RUSTDOC-STORE-SUBMODULE-LINKS-UNRESOLVED` | draft | P3 | Unresolved intra-doc links render as text, not as broken `href`s | no | no | None |

## Interfaces

All in `liquers-docs/src/`. Serde derives on the configuration types; no `ExtValue` variant.

```rust
// site.rs — docs/site.yaml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub title: String,
    #[serde(default)] pub pages: Vec<PageRule>,         // first matching rule wins
    #[serde(default)] pub generate: Vec<GeneratedPages>,
    #[serde(default)] pub packages: Vec<Package>,
    #[serde(default)] pub copy: Vec<String>,            // globs copied verbatim (assets/**)
    #[serde(default)] pub exclude: Vec<String>,         // README.md, templates/**
    pub registry: Option<String>,                       // path to command_registry.yaml, relative to docs/
}
pub struct PageRule { pub pattern: String, pub query: Option<String> }   // None: not a page
pub struct GeneratedPages { pub each: String, pub path: String, pub query: String }
pub struct Package { pub language: String, pub name: String, pub path: String }
pub struct GuideInfo { pub title: String, pub audiences: Vec<String>, #[serde(default)] pub languages: Vec<String>, #[serde(default)] pub order: i64 }

// build.rs
pub struct BuildOptions { pub source: PathBuf, pub out: PathBuf, pub check_links: bool }
pub struct BuildReport { pub pages: usize, pub copied: usize, pub warnings: Vec<String> }
pub async fn build_site(options: &BuildOptions) -> Result<BuildReport, Error>;
pub async fn check_pages(source: &Path) -> Result<Vec<String>, Error>;   // ns-template/validate on every page

// links.rs: a synchronous file walk, run inside tokio::task::spawn_blocking from build_site
pub struct BrokenLink { pub page: PathBuf, pub target: String }
pub fn check_links(out: &Path) -> Result<Vec<BrokenLink>, Error>;

// commands.rs
pub fn register_docs_commands(cr: &mut CommandRegistry<DocsEnvironment>) -> Result<(), Error>;
```

`PageRule.query` and `GeneratedPages.query` / `path` are Liquers **query templates**, using the
`liquers-template` query-template form with hygienic slots. A page rule receives `{{key}}`; a
generated page receives `{{.}}`, one item of the list `each` names in the site data. The default
rules:

```yaml
pages:
  - { pattern: "**/*.md", query: "-R/{{key}}/-/ns-template/expand-~X~ns-docs/site~E" }
generate:
  - { each: namespaces, path: "reference/commands/{{.}}.md", query: "ns-docs/commands-{{.}}" }
```

Commands, namespace `docs`:

```rust
register_command!(cr, async fn site(state, context) -> result
    namespace: "docs" label: "Site data" volatile: true
    doc: "site.yaml, every guides/*/guide.yaml, packages with `built`, command namespaces")?;
register_command!(cr, fn md_to_html(state) -> result
    namespace: "docs" label: "Markdown to HTML" filename: "page.html")?;
register_command!(cr, async fn commands(state, namespace: String, context) -> result
    namespace: "docs" label: "Command reference" filename: "commands.md")?;
```

- **`site`** returns `{title, guides: [{name, title, audiences, languages, href}], audiences:
  [{name, guides}], packages: [{language, name, href, built}], namespaces: [..]}`. It reads
  through the store, so files become dependencies. `built` checks the package path under the
  read-only `_site` mount. It is volatile because it depends on generator output written outside
  the store.
- **`commands`** formats one namespace from the registry file. It reuses the formatter extracted
  from `liquers-lib`'s `commands_doc` into `pub fn commands_markdown(registry:
  &CommandMetadataRegistry, namespace: &str, name: &str) -> String`, which `commands_doc` then
  calls. It reads the file with `liquers_core::validate::registry::from_json_or_yaml`.
- **`md_to_html`** uses pulldown-cmark with tables, footnotes and strikethrough. It rewrites a
  relative `x.md` link to `x.html`, so the Markdown sources also read correctly on GitHub.

Sync or async: `site` and `commands` read the store (async). `md_to_html` is pure (sync). The layout
is rendered with `liquers_template::render` and a `MapHost` holding `templates/` partials, with the
data `{title, body, root, nav, site}`. `root` is the page's relative path to the site root
(`../../`).

## Integration Points

- **New crate `liquers-docs`:**
  - `Cargo.toml` depends on `liquers-core`, `liquers-store`, `liquers-lib` (`default-features =
    false, features = ["templates"]`), `liquers-template`, `liquers-macro`, `pulldown-cmark` 0.12
    (`html`), `serde`, `serde_yaml`, `clap`, `tokio`.
  - `src/main.rs` follows the `liquers-agent/src/main.rs` pattern. `src/lib.rs` provides
    `build_environment`, as in `liquers-agent/src/lib.rs`: `default_environment_builder` plus
    `register_all_commands!` plus `register_docs_commands`, with `with_config` mounting `docs/`
    read-only at the root and the output folder read-only at `_site`.
- **Workspace `Cargo.toml`:** add `liquers-docs` to `members`, not to `default-members`.
- **`liquers-lib/src/commands.rs`:** extract `commands_markdown`. `commands_doc`'s output must not
  change, and `registry_export` keeps passing.
- **`scripts/build-docs.sh [--out DIR] [--skip-api] [--no-link-check]`**, default output
  `target/docs-site/`:
  1. `cargo doc --no-deps --workspace --exclude liquers-py --exclude liquers-web` (their users
     read the Python and JavaScript references), copied to `api/rust/`.
  2. Python and JavaScript generators when present.
  3. `cargo run -p liquers-docs -- build`.
- **`.github/workflows/docs-site.yml`:** runs on PRs touching `docs/**`, `liquers-docs/**` or the
  script, and weekly. It uploads the site as an artifact.
- **`docs/site.yaml`:** gets the schema above. Its file is created by `api-documentation-rules`.

## Error Handling

| Failure | Behaviour | Constructor |
|---|---|---|
| `site.yaml` or a `guide.yaml` does not parse | Build fails, naming the file | `Error::from_error(ErrorType::General, e)` via `from_json_or_yaml` |
| A page's template or query fails | Build fails with the page key and the template error's position (`errors: strict`) | The `ns-template` error, passed through |
| The registry file is missing | `ns-docs/commands` fails, so the generated pages fail | `Error::general_error` naming the path |
| A broken relative link | Every link is listed, and the build exits 1 | Not an `Error`; a `BrokenLink` list |
| An output write fails | Build fails, naming the path | `Error::from_error(ErrorType::General, io_error)` |
| A generator is missing (script) | Notice on stderr, step skipped, `built: false` | Shell |

The binary prints diagnostics to stderr and only the summary to stdout. Exit codes: 0 built, 1 a
page or link failed, 2 usage.

## Relevant Commands

New namespace `docs`: `site`, `md_to_html`, `commands`. Existing ones used: `template` (`expand`,
`validate`). `commands_doc` keeps its signature. The `ns-docs` commands are registered by
`liquers-docs`, not by `liquers-lib`, so they are not in `specs/command_registry.yaml`, and
`liquers-validate` sees them only with `--command`. `liquers-docs check` validates pages against
the real registry.

## Documentation Architecture

| Path | Kind | Audience | Area | Change |
|---|---|---|---|---|
| `liquers-docs/src/lib.rs` | API doc | user | docs | module doc per `API_DOCUMENTATION_GUIDE.md` |
| `docs/README.md` | — | internal | docs | the build and the `site.yaml` schema |
| `guides/TEMPLATE_GUIDE.md` | guide | internal | docs | §3 or §4: the site as a worked example; History |
| `CLAUDE.md` | — | — | docs | Structure: `liquers-docs/`; Building: `scripts/build-docs.sh` |
| `specs/README.md` | map | internal | docs | the tool under "When to use what" |

Proposed `affects_docs`: `guides/TEMPLATE_GUIDE.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | New crate (~6 files), `Cargo.toml`, `liquers-lib/src/commands.rs`, the script, a workflow, `docs/site.yaml`, `docs/templates/page.html`, 4 docs |
| Crates and workflows affected | `liquers-docs` (new), `liquers-lib` (one extracted function), a new workflow |
| Existing tests likely to change | None; `registry_export` must still pass |
| New validation | Unit tests per command, `build_site` over a fixture folder in a memory store, link checker tests, an end-to-end build of `docs/` in CI |
| Compatibility / data / concurrency / performance / security | `cargo doc` of the workspace with default features builds polars and egui metadata: CI time and disk, measured in Phase 3. The docs mount is read-only. Templates use `{{$ }}` only, never `{{$$ }}` |
| Recovery | The crate is not a default member, so removing it touches nothing else |
| Certainty and open questions | Medium. Whether `expand`'s HTML escaping stays off for `.md` input (it is on only for `.html` filenames, `TEMPLATE_LANGUAGE.md` §4), and the rustdoc build cost, are checked in Phase 3 |
