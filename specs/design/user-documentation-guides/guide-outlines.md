# The three guides: structure, content proposal and editorial markers

Part of Phase 1 of `user-documentation-guides` (the owner's question 1, 2026-11-01). This file
defines what the skeleton will contain. It is not the guides' text.

## 1. Model

The model follows the old LiQuer user guide (<https://orest-d.github.io/liquer/site/index.html>).
That guide had a homepage with a hello-world and "next steps", then a single *User Guide* of short
ordered chapters (Introduction, Query syntax, Store and Cache, Recipes, Metadata and state,
Commands, Web service, Security, Examples), then an *API* link. What carries over: a hello-world
on the first page, one concept per chapter in teaching order, and a closing examples chapter.
What changes: three guides instead of one, and a shared chapter skeleton.

Three guides, one per way of using Liquers:

| Guide | Folder | Reader | Integration |
|---|---|---|---|
| *Liquers for Rust* | `docs/guides/rust/` | Rust developers building on the crates, writing commands, value types, stores, servers | the crates |
| *Liquers for Python* | `docs/guides/python/` | data scientists and Python developers | `liquers-py` |
| *Liquers for the Web* | `docs/guides/web/` | front-end developers running Liquers in a browser page | `liquers-web` (JavaScript, wasm) |

"Web" here means the browser integration. Serving Liquers over HTTP is a chapter of the Rust guide
(`liquers-axum`), and the HTTP API is an appendix of the Web guide, because a page may call a
server instead of running Liquers itself.

## 2. One chapter skeleton for all three

Every guide has the same chapters with the same numbers and the same file names. So
"chapter 5 of the Python guide" and "chapter 5 of the Rust guide" are about the same thing, and a
chapter can say "the same in Rust: `[5. Commands](../rust/05-commands.md)`". When a chapter does
not apply to a language, it keeps its number and holds one paragraph saying why and where to look
instead. That paragraph keeps the cross-references valid.

A chapter's language-neutral explanation (the query language, what an asset is, the glossary) is
written once in `docs/guides/_shared/` and included by all three guides as a partial. Only the
language-specific parts are written three times.

```
docs/guides/
  index.md                  the three guides side by side, and how they relate
  _shared/                  language-neutral fragments included by all three (not pages)
  rust/  python/  web/
    guide.yaml              title, audience, chapter order, formats, status
    index.md                preface
    01-introduction.md … 15-troubleshooting.md
    a-query-syntax.md  b-command-library.md  c-glossary.md  d-sister-guides.md
    HISTORY.md              review history of the guide's chapters
```

| # | Chapter | Rust | Python | Web | Draft from (`specs/`) |
|---|---|---|---|---|---|
| 0 | Preface | who it is for, how to read it, the conventions, the sister guides | same | same | `VISION.md`, `POSITIONING.md` |
| 1 | Introduction | what Liquers is; hello-world: register a command, evaluate a query | the same with `@command` | the same in a page | `PROJECT_OVERVIEW.md`; `async_hellow_world.rs`; `PYTHON_GUIDE.md` §3; `JAVASCRIPT_GUIDE.md` §3 |
| 2 | Installing and setting up | crates, features, MSRV | `pip`, `maturin develop` | wasm build, loading the module | `CLAUDE.md` build notes; `PYTHON_GUIDE.md` §1-2; `JAVASCRIPT_GUIDE.md` §1-2 |
| 3 | Queries | *shared*: the query language, keys, resources, actions, escaping; then running a query in the language | | | `PROJECT_OVERVIEW.md`; `QUERY_ESCAPING_GUIDE.md`; `DOC_02_QUERY_LANGUAGE_REFERENCE.md` |
| 4 | Values, state and metadata | `Value`, `ExtValue`, `State`, `Metadata` | conversion to and from Python objects | conversion to and from JavaScript | `VALUE_TYPE_SYSTEM.md`; `TYPE_SYSTEM_GUIDE.md`; `PYTHON_GUIDE.md` §7; `JAVASCRIPT_GUIDE.md` §7 |
| 5 | Commands | `register_command!`, its DSL, namespaces, defaults, context | the decorator, composition | object-literal declarations | `COMMAND_REGISTRATION_GUIDE.md`; `COMMAND_DESIGN_GUIDE.md`; `COMMAND_DECLARATION.md`; guides §5 |
| 6 | Stores | the store trait, backends, configuration documents, metadata pipeline | configuring and defining stores | browser stores (`localStorage`, fetch) | `STORE_CONFIG_GUIDE.md`; `STORE_SEMANTICS.md`; `STORE_IMPLEMENTATION_GUIDE.md`; `JAVASCRIPT_GUIDE.md` §9 |
| 7 | Recipes, assets and caching | *shared*: what an asset is, its lifecycle, dependencies, expiration; then the language API | | | `ASSETS.md`; `ASSET_LIFECYCLE.md`; `RECIPES_AND_PLANS.md`; `DEPENDENCY_CONSISTENCY_GUIDE.md` |
| 8 | The environment | `EnvironmentBuilder`, configuration documents | building an environment from Python | the environment in a page | `ENVIRONMENT_CONSTRUCTION_GUIDE.md`; `ENVIRONMENT_CONFIG.md`; guides §4 |
| 9 | Working with data | polars, record streams, images | data frames, records | record streams in the browser | `POLARS_COMMAND_LIBRARY.md`; `RECORD_STREAM_GUIDE.md`; `IMAGE_COMMAND_LIBRARY.md`; `JAVASCRIPT_GUIDE.md` §10 |
| 10 | Templates | *shared*: Mustache with query tags; then rendering from the language | | | `TEMPLATE_GUIDE.md`; `TEMPLATE_LANGUAGE.md` |
| 11 | User interfaces | `UISpec`, egui and the web UI | not available (paragraph + pointers) | mounting and defining UI elements | `UI_INTERFACE_FSD.md`; `JAVASCRIPT_GUIDE.md` §11 |
| 12 | Serving and embedding | the HTTP server (`liquers-axum`), WebSocket events | embedding in notebooks and services | calling a server from a page | `WEB_API_GUIDE.md`; `WEB_API_SPECIFICATION.md` |
| 13 | Search and agent memory | search, records, the agent memory and MCP | using the memory from Python | not available (paragraph) | `SEARCH_GUIDE.md`; `AGENT_MEMORY_SERVICE.md` |
| 14 | Testing | unit tests, memory stores, fixtures | pytest with `liquers` | Node and browser tests | `UNITTEST_GUIDE.md`; guides §9/§12 |
| 15 | Troubleshooting | *shared* error kinds; then the language's error mapping, known limitations | | | `error.rs` docs; guides §8, §10/§13 |
| A | Query syntax | *shared* reference, included from `specs/` | | | `PROJECT_OVERVIEW.md` query sections |
| B | Command library | generated from `specs/command_registry.yaml` | | | the registry |
| C | Glossary | *shared* | | | `liquers-core/src/lib.rs` glossary |
| D | Sister guides | chapter-by-chapter map of the other two guides | | | — |

*Shared* means the language-neutral part is a `_shared/` fragment. The table's "Draft from"
column becomes each chapter's `sources:` front matter (`USER_DOCS_EDITING_GUIDE.md`, maintenance), which is
what links a chapter to the specs a design may change.

## 3. Content proposal per chapter

Each skeleton chapter contains:
- its title;
- one `PURPOSE:` paragraph (what the reader can do after it);
- a `TBD:` list of the sections to write, each with a one-line content proposal and its source;
- `SOURCE:` lines;
- for chapter 1, the hello-world copied from the named test that runs it.

For example, the skeleton of `python/05-commands.md`:

```markdown
---
title: Commands
status: outline
sources: [guides/PYTHON_GUIDE.md, guides/COMMAND_REGISTRATION_GUIDE.md, reference/COMMAND_DECLARATION.md]
reviewed:
---
# 5. Commands

PURPOSE: The reader can turn a Python function into a Liquers command, choose its namespace and
defaults, and call it from a query.

TBD: 5.1 A function becomes a command: `@command`, the state argument, what introspection fills in.
SOURCE: guides/PYTHON_GUIDE.md §5
TBD: 5.2 Arguments and defaults: types, defaults, links (`query "…"`); only what differs from the defaults.
SOURCE: reference/COMMAND_DECLARATION.md
TBD: 5.3 Namespaces and realms.
TBD: 5.4 The context: logging, progress, evaluating other queries.
EDITOR: keep 5.1-5.4 parallel to rust/05-commands.md and web/05-commands.md.
```

## 4. Editorial markers

A line or paragraph that starts with one of these words, in capitals and followed by a colon, is
**not reader text**:

| Marker | Meaning | Draft HTML build | Release build |
|---|---|---|---|
| `TBD:` | content still to be written; says what | shown as a highlighted box | **fails** if present in a chapter whose `status` is `ready` |
| `PURPOSE:` | what the chapter enables; guides the writer | shown, muted | removed |
| `SOURCE:` | where to draft from (`specs/` path and section) | shown, muted | removed |
| `EDITOR:` | a note or instruction to editors | shown, muted | removed |
| `CHECK:` | a claim to verify against the code before release | shown, highlighted | **fails** in a `ready` chapter |

Multi-paragraph notes use a fenced div with the marker as its class (`::: tbd` … `:::`), which is
Pandoc/Quarto syntax and renders as plain text on GitHub. The build tool lists every marker per
guide (`TBD` report), so the remaining work is always visible. The markers never appear in a
`specs/` include.

Chapter `status`: `outline` (skeleton only), `draft` (text written, markers allowed), `review`
(waiting for a person), `ready` (published in release builds; no `TBD:` or `CHECK:`). A release
build publishes `ready` chapters and shows every other chapter as a one-line "in preparation"
entry in the table of contents. A draft build publishes everything.
