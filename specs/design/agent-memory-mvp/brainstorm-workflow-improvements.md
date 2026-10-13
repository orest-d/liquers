# Brainstorm — what the memory service could do for the Liquers workflow

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-12. A brainstorm, not a
decision record: it collects ideas, measures what it can, and ranks. What Phase 1 adopts from it is
listed in §7. It builds on the earlier analyses — [survey](./memory-systems-survey.md),
[specification](./service-specification.md), [workflow porting](./workflow-porting.md),
[tiers](./tiers-and-storage.md), [interfaces](./interfaces-and-mcp.md) — and on three things that
landed on `main` since they were written:

- **Search is merged** (`store-and-asset-search` complete): `ns-search/catalog`, `field_extract`,
  `md_split`, `commands`, `search` with BM25, regex and field tests.
- **Argument overrides** (`axum-query-named-arguments`, `guides/WEB_API_GUIDE.md` §4): arguments of
  a query's last action can be passed by name as URL parameters or a JSON body, with no escaping;
  such an evaluation is *unmanaged* (not cached). In-process the same mechanism is
  `override_value` / `override_link` on the plan (`liquers-core/src/plan.rs`). An MCP tool's JSON
  arguments map onto it one to one, which removes the escaping problem `SEARCH_GUIDE.md` §4
  describes.
- **Store configuration** (`reference/STORE_CONFIG_FSD.md`): a declarative router of mounts — file
  system, memory, S3 and the other OpenDAL services — with `${VAR}` expansion for credentials.

## 1. What the current workflow costs, measured

At `main` on 2026-10-12 (sizes in bytes; ~4 bytes per token):

| Corpus part | Files | Size |
|---|---|---|
| `specs/issues/` | 301 | 1.15 MB |
| `specs/design/` | 896 (in 170 folders) | 8.85 MB |
| `specs/reference/` | 26 | 0.84 MB |
| `specs/guides/` | 16 | 0.54 MB |
| `specs/archive/` | 44 | 0.74 MB |
| `.claude/skills/` | 22 | 0.14 MB |
| Rust sources | 362 files, 181k lines | 7.2 MB |

| What an agent reads, and when | Size | Approx. tokens |
|---|---|---|
| `CLAUDE.md`, every session, whole | 25 KB | ~6k |
| `DOCS_STRUCTURE_GUIDE.md`, to file or triage an issue (often whole) | 69 KB | ~17k |
| `specs/index.csv`, grepped; read whole when the grep is unclear | 122 KB | ~30k |
| `specs/README.md` | 38 KB | ~9k |
| One design folder at Phase 2 (typical, all phases) | 30–150 KB | 8–40k |
| `liquers-project` skill + references at a phase gate | ~60 KB | ~15k |

Open work is concentrated: of the active issues, `core/assets` has 27, `core/store` 19, `web` 16,
`core/value` 14, `axum` 12. An agent starting on any of them needs "the relevant reference
sections, the guides, the open issues and designs in that area, and the code" — today assembled by
hand, by grep, by reading whole files.

The pattern in every row: **the decision needs an L0/L1, the agent pays for an L2**. That is the gap
the tiers, search and context packs below close.

## 2. One engine, several shapes — configuration

The brief: the MVP must be configurable either like the current manual Liquers system or in
OpenViking's style. Both are the same five things in different arrangements, so a memory is
configured by one document with five sections:

| Section | Says | Built on |
|---|---|---|
| `stores` | Which directories and remote stores make up the memory, under which key prefixes, read-only or writable | `StoreRouterConfig` as it is today (`STORE_CONFIG_FSD.md`) — reused verbatim |
| `collections` | Folders with a kind: schema, id rule, template, tier fallbacks, check rules | `_schema/` documents (`service-specification.md` §2.3), or inline |
| `tiers` | Where L0/L1 come from: front-matter, a named section, first heading/paragraph, a folder README, or a generated summary | `tiers-and-storage.md` §3 |
| `views` | Saved queries with names: "open issues by area", "eligible for autofix", "designs in review" | Recipes (`recipes.yaml`) |
| `tools` | Which MCP tool sets are offered and with which defaults | `interfaces-and-mcp.md` §6 |

### 2.1 Profile "liquers-specs" — the current manual system, served

```yaml
# memory.yaml — this repository
stores:                       # order matters: the more specific prefix first
  - { type: filesystem, prefix: specs/archive, config: { path: ./specs/archive }, read_only: true }
  - { type: filesystem, prefix: specs,  config: { path: ./specs } }
  - { type: filesystem, prefix: skills, config: { path: ./.claude/skills }, read_only: true }
  - { type: filesystem, prefix: src,    config: { path: . }, read_only: true }   # code, see §5.2
  - { type: s3, prefix: notes, config: { bucket: "${MEM_BUCKET}", root: "liquers/notes",
      access_key_id: "${AWS_ACCESS_KEY_ID}", secret_access_key: "${AWS_SECRET_ACCESS_KEY}" } }
collections:
  issues:  { folder: specs/issues,  schema: _schema/issues.yaml,  template: _templates/issue.md,
             l1: { section: Problem } }
  designs: { folder: specs/design,  unit: folder, entry: DESIGN.md, schema: _schema/designs.yaml,
             l1: { file: phase1-high-level-design.md, section: Purpose } }
  reference: { folder: specs/reference, l1: { first_paragraph: true } }
  guides:  { folder: specs/guides,  l1: { first_paragraph: true } }
  skills:  { folder: skills, entry: SKILL.md, l0: name, l1: description }
  notes:   { folder: notes, schema: _schema/notes.yaml }       # agent-written, survives sessions
views:
  index.csv:      "-R-key/specs/-/ns-mem/index_records/ns-rec/to_csv"
  open-by-area:   "-R-key/specs/issues/-/ns-search/field_extract/search-…"
tools: [memory]
```

`filesystem` takes `path`; the S3 type takes `bucket`, `root` and credentials
(`STORE_CONFIG_FSD.md`). The router takes the first mount whose prefix matches, hence the order;
whether a listing of `specs/` shows the nested `archive` mount is checked in Phase 2. `read_only` is
a proposed per-mount flag, not an existing field (`STORE-NO-READ-ONLY-ADAPTER`); `unit: folder` makes a
design folder one memory whose entry document carries the front-matter; `l1:` is the fallback rule
the corpus needs because no document has a `description:` today. Query forms here are illustrative.

### 2.2 Profile "openviking" — a context database for an agent

```yaml
stores:
  - { type: filesystem, prefix: resources, config: { path: ./docs },  read_only: true }
  - { type: s3, prefix: user,   config: { bucket: "${MEM_BUCKET}", root: "u/${USER_ID}" } }
  - { type: s3, prefix: agent,  config: { bucket: "${MEM_BUCKET}", root: "agent" } }
collections:
  profile:     { folder: user/memories, file: profile.md, unit: single }
  preferences: { folder: user/memories/preferences }
  entities:    { folder: user/memories/entities }
  events:      { folder: user/memories/events, fields: { date: { type: date, required: true } } }
  cases:       { folder: agent/memories/cases }
  skills:      { folder: agent/skills, entry: SKILL.md, l0: name, l1: description }
tiers:
  default: { front_matter: true, fallback: first_paragraph }
  folders: { readme: .overview.md }          # OpenViking's sidecar name, as a folder README
tools: [memory]
```

The differences from §2.1 are data: category folders instead of genres, a per-user prefix, folder
overviews named `.overview.md`. What OpenViking does with a language model (generate overviews,
extract memories at session commit) stays agent-side in the MVP (principle P2) and becomes
model-backed commands later.

### 2.3 What configuration buys

- **The same tools on both.** `memory_search` does not know which profile it serves.
- **Remote, durable agent memory.** A Claude Code cloud container is ephemeral; an S3 mount for
  `notes/` (or `user/`) is how what an agent learned survives the session without being committed.
  Caveat: the environment's network policy must allow the S3 endpoint, and the credentials come
  from the environment's secrets — the default policy in this container does not reach arbitrary
  hosts (§6).
- **Shapes mapped onto existing directories.** The router mounts `./specs` as it is; nothing is
  moved. Adopting the service is configuration, not a migration.
- **Several memories in one session.** A project memory (`specs/`) and a personal one (`notes/`) are
  two mounts of one router, searched together or apart by prefix.

## 3. Workflow improvements

Each entry: today, with the service, and the tool. Ordered by how often the task occurs.

### W1. Orientation at session start

Today: `CLAUDE.md` (6k tokens) is loaded; anything else is found by grep. With the service:
`memory_overview` returns the memory's L1, collection counts, open P0/P1 issues, designs at a gate
and the last log lines — about 1k tokens — and `CLAUDE.md` can shrink to rules and pointers, with
the reference material moved behind `memory_read`.

### W2. Filing an issue

Today (`DOCS_STRUCTURE_GUIDE.md` §4.8): grep `index.csv`, read candidates whole, run
`docs_index.py new`, which repeats a word-overlap search and refuses without `--force`, then fill
the body. With the service: `memory_new(collection=issues, title, …)` returns BM25-ranked candidates
with L1 and excerpt and writes nothing; a second call with `force` writes the instantiated template
(§4) with vocabulary-checked fields, stamps `created`, logs the write and refreshes `index.csv` as an
asset. The agent never opens `DOCS_STRUCTURE_GUIDE.md` to remember the field list — `memory_describe`
gives it.

### W3. "Is this already known?" and overlap triage

Today: grep, then the four overlap tests of `references/overlap.md` done by reading. With the
service: `memory_search` scoped to `specs/issues` and `specs/design` with `-status:closed*`, then
`memory_related(key)` — documents that link to it, that it links to, that share its `area`, and its
nearest neighbours by text (and by meaning once §5.3 lands). The overlap judgement stays the agent's;
the candidate list is one call.

### W4. Assembling the relevant documentation — context packs

The biggest gain. Today an agent starting on a `core/store` issue reads `STORE_SEMANTICS.md`,
`STORE_IMPLEMENTATION_GUIDE.md`, `STORE_CONFIG_FSD.md`, the issue, related issues and designs, and
the code, mostly whole. With the service, a **context pack** is one document assembled from queries:

```markdown
# Context: {{ issue.title }}
{{ issue.description }}

## Reference sections that mention {{ symbols | join(", ") }}
$-R-key/specs/reference/-/ns-search/md_split/search-…/ns-mem/to_md$

## Open issues in {{ issue.area | join(", ") }}
$-R-key/specs/issues/-/ns-search/field_extract/search-…/ns-mem/to_md_list$

## Code items
$-R-key/src/-/ns-mem/rust_items/search-…/ns-mem/to_md$
```

- Built by `memory_context(key | topic | files, budget)`: it takes the issue's `area`, the symbols in
  backticks, the files named in it, and fills a pack template (§4) whose sections are searches.
- **Budgeted**: each section has a limit; the pack ends with "omitted: N more, ask with …", so a cut
  is never silent (the search design's rule).
- **An asset**: the pack is a recipe with dependencies on what it embedded, so it is cached and
  recomputed when an embedded document changes — which requires the template evaluator to record
  dependencies (`SIMPLE-TEMPLATE-HAS-NO-COMMAND`, filed 2026-10-12).
- Packs are useful to people too: a reviewer's pack for a PR, a newcomer's pack for an area.

### W5. Design work

Today: `init_feature.py`, templates extracted from fenced blocks, `validate_phase.py`,
`docs_index.py --check`, readiness bookkeeping by hand. With the service: `memory_new(designs, …)`
instantiates the folder template; `memory_check(path)` runs the phase rules, AC coverage and
readiness reciprocity; a `designs-at-gate` view lists what waits for approval. Phase content stays
judgement.

### W6. Review and documentation upkeep

`memory_check` reports: reference documents overdue (`reviewed:` > 92 days), a top `## History` row
not matching `reviewed:`, dead links, and — new, with code records (§5.2) — **code references that
no longer resolve**: a design or reference cites `liquers-core/src/assets.rs` `AssetManager::get`,
and the symbol has moved or gone. The skill's rule "refer to code by path and symbol" becomes
checkable, which turns doc drift from a quarterly review into a check.

### W7. Fixing an issue

`memory_context(issue)` (W4) plus the `autofix-eligible` view replaces the CSV query in
`references/auto-fix.md`. After the fix, `memory_edit(set_field status closed)` refuses a transition
the schema does not allow and logs it.

### W8. What did previous sessions learn?

Today nothing survives a cloud session except commits; a finding not filed is lost (which is why
`CLAUDE.md` insists on filing). With the service: `notes/` (a durable mount) holds findings,
dead ends and session summaries (`episodes/`), each with L0/L1, searchable by the next session. An
agent can record "tried X, failed because Y" without the ceremony of an issue — and promote it to an
issue when it turns out to be one.

### W9. Command and query discovery

`ns-search/commands` already answers "which command does X"; through MCP it is a `memory_search` over
a `commands` collection whose records come from the registry. `liquers-validate` stays a CLI (and a
general-MCP tool later).

### W10. Backlog compaction and bulk design

The mechanical steps of `references/compaction.md` — inventory, filters, decision-list aggregation —
are views. The overlap map is `memory_related` over the open set. Judgement unchanged.

## 4. Templating

Three templating needs, with different shapes:

| Need | Shape | Example |
|---|---|---|
| T-a Instantiate a document | Fields into a skeleton; loops over lists; defaults | Issue and design templates; note templates |
| T-b Embed query results | A document whose parts are other assets | Context packs; README generated blocks; reports |
| T-c Render records as text | A table or list from records | `index.md`; the sections of a pack |

Candidates:

| Mechanism | What it is | T-a | T-b | T-c | Notes |
|---|---|---|---|---|---|
| **`simple_template`** (`liquers-core/src/parse.rs`, `interpreter.rs` `evaluate_simple_template`) | Text with `$<query>$` spans; `$$` is a literal `$` | ✗ no variables | ✔ the right primitive | ✗ needs text results | In core, wasm-safe. Unused by any command; evaluates spans outside the asset manager and drops their dependencies (`SIMPLE-TEMPLATE-HAS-NO-COMMAND`). Every `$` in prose must be doubled |
| **`expand`** (ranees-liquers) | Command: state = template text, `simple_template` evaluated with the cwd | ✗ | ✔ | ✗ | Proven: ranees pages include menus with `$-R/…/menu.yaml/-/…/mustache-menu.html$`. Port it, fixing the evaluator |
| **`mustache`** (ranees-liquers, crate `mustache` 0.9) | Command: state = data model, template by name; then optionally `expand` | ✔ logic-less | ✔ via second pass | ✔ loops | Rendering first and expanding second lets data *generate* queries — powerful. The crate is old and unmaintained; numbers become strings; no store-backed partials |
| **minijinja** (new) | Jinja-like engine, pure Rust, wasm-compatible, serde data model | ✔ conditionals, filters, `default`, strict undefined | ✔ two-pass, or pre-evaluated queries in the context | ✔ loops, `join`, custom filters | A store-backed loader gives includes; maintained; moderate weight. Its rendering is synchronous, so queries are evaluated before or after, not during |

> **Superseded 2026-10-12** by [`template-language-evaluation.md`](./template-language-evaluation.md):
> the user asked for the most *suitable* language to reimplement (query-aware), not the most
> powerful. The evaluation recommends Mustache with query tags, and a Mustache-compatible interim
> instead of minijinja. The proposal below is kept as written.

**Proposal: two layers, as ranees had, with a modern engine.**

1. **`expand`** in `liquers-lib`: `simple_template` evaluated through the environment's asset
   manager, spans in parallel, their dependencies and volatility recorded on the result. This is T-b
   and makes packs assets.
2. **`render-<template key>`** (feature `templates`, minijinja): data model = the state as JSON
   (records via `ns-rec/to_json`), plus named arguments; a loader reading templates from the store;
   optional second pass through `expand`. This is T-a and T-c.
3. **Small text converters** for T-c without a template: `to_md_table`, `to_md_list` over records.
4. **`replace_block-<name>`** for generated blocks inside hand-written files (`<!-- BEGIN generated:
   … -->`), which neither engine does.

The front-matter of an instantiated template is data too: `memory_new` validates the rendered
front-matter against the schema before writing, so a template cannot produce an invalid memory.

Mustache is not proposed: minijinja covers its use with a maintained crate, and the ranees templates
are few enough to port. If compatibility matters, minijinja's syntax covers mustache's `{{ x }}`
and sections differ only in spelling.

## 5. Features that would make it more useful

### 5.1 Ranked

Value: how much of §3 it unlocks. Cost: build and code. Cloud fit: works in a fresh, network-limited
container (§6).

| # | Feature | Value | Cost | Cloud fit | Stage |
|---|---|---|---|---|---|
| F1 | **Markdown navigation**: outline, section by heading path, links out and in (`NO-MARKDOWN-NAVIGATION-COMMANDS`) | High — W3, W4, W6 | M | ✔ | MVP (minimum: section, links) |
| F2 | **Context packs** (`memory_context`, `expand`, converters) | High — W4, W7 | M | ✔ | MVP |
| F3 | **Saved views** as recipes | High — W5, W7, W10 | S | ✔ | MVP |
| F4 | **Rust item records** (`syn`) — §5.2 | High — W4, W6 | M | ✔ (~1 s, no compile) | MVP or right after |
| F5 | **Code-reference check** (doc ↔ code drift) on F4 | High — W6 | S on F4 | ✔ | With F4 |
| F6 | **Templates** (`render`; language per `template-language-evaluation.md`) | Medium — W2, W5 | S–M | ✔ | MVP for instantiation |
| F7 | **Durable notes mount** (S3 or committed) | Medium — W8 | S | Needs network policy for S3 | MVP as configuration |
| F8 | **Hybrid semantic search** (static embeddings + BM25, RRF) — §5.3 | Medium — W3, W4 | M | ✔ only with vendored weights | NEXT |
| F9 | **Write log and recent activity** | Medium — W1, W8 | S | ✔ | MVP |
| F10 | **Git awareness**: changed since a commit, last change per document | Medium — W1, W6 | S (git CLI) | ✔ | NEXT |
| F11 | **References and call graph** (`rust-analyzer scip`) — §5.2 | Medium | L | ✘ per session (169 s, 7 GB); ✔ in CI | Later |
| F12 | **Concept / glossary pages** (LLM-wiki entities: Asset, Recipe, Key…) linked from documents | Medium | S content, M upkeep | ✔ | Later, agent-written |
| F13 | **Model-backed summaries / extraction** (MCP sampling or a provider) | Low now | M | ✘ no API reachable here | Later |
| F14 | **Vector database** (HNSW, Qdrant, LanceDB) | Low at this scale — §5.3 | M–L | Mixed | Not needed |

### 5.2 Rust syntax understanding

What an agent asks of code here: where is X defined, what does module M export, what is the
signature and doc of X, which items in this file, who calls X. The options, measured in this
container on 2026-10-12:

| Option | Yields | Cost here | wasm |
|---|---|---|---|
| **`syn`** item extraction | Every item: crate, file, module path, kind, name, signature (body elided), doc comment, attributes/`cfg`, line range, visibility, impl target | `syn` 2 with `full` is already a dependency (`liquers-macro`); parsing 7 MB ≈ 1 s; no compilation of the workspace | ✔ |
| tree-sitter-rust | Syntax tree, error-tolerant, multi-language; name-based tags | C build | ✘ without a custom sysroot |
| `rust-analyzer scip` | Every occurrence, definitions and references — a call graph | **169 s, 7 GB RSS, 35 MB index** on this workspace | ✘ |
| rustdoc JSON | Resolved public API, impls, trait relations | Nightly only; a type-check per crate | ✘ |
| `ra_ap_*` as a library | Full semantics | Very heavy build, unstable API | ✘ |

**Proposal:** `ns-mem/rust_items` — a record producer over a folder of `.rs` files using `syn`, one
row per item with the fields above and a `body_hash`. It composes with `ns-search` at once ("fn named
`get_asset_info` in `liquers-core`" is a field test, not a regex that also hits calls and comments),
feeds packs (W4) and makes the code-reference check (F5) possible. It closes the first half of
`NO-SOURCE-CODE-AND-API-DOC-NAVIGATION`. References and the call graph come later from SCIP produced in
CI, never per session.

This is also how the leading agent tools split it: Aider builds a repository map from syntax tags
ranked by references; Serena delegates to a language server; Claude Code itself uses no index —
agentic grep and read. A syntax-level item table is the cheap middle that a grep cannot give.

### 5.3 Semantic search and vector databases

**Is it needed?** Lexical search fails on paraphrase — "how do I add a storage backend" finds
`STORE_IMPLEMENTATION_GUIDE.md` only if the words match. Hybrid search (BM25 + embeddings, fused by
reciprocal rank) is the measured sweet spot for code plus docs: MinishLab's Semble reports
NDCG@10 0.854 over ~1,250 queries in 63 repositories with BM25 + a 16M-parameter static embedding
model, about on par with a 137M-parameter transformer, at ~1 ms per query. But lexical search plus
good L0/L1 plus an agent that can rephrase covers most of it — Claude Code chose agentic search over
a vector index. So: valuable, not first.

**What runs here.** The network policy in this container blocks `huggingface.co`, the ONNX Runtime
download (`cdn.pyke.io`) and the embedding APIs (OpenAI, Voyage, Cohere); `static.crates.io`,
`static.rust-lang.org`, PyPI, npm and git are reachable. Therefore:

| Option | Fit |
|---|---|
| **Static embeddings, `model2vec-rs`** (`potion-code-16M` for code, `potion-base-8M` for prose) | ✔ pure Rust, wasm-capable, ~8,000 texts/s on one core (≈5 s for 40k chunks); weights **vendored** (8–30 MB, quantized) because they cannot be downloaded at run time |
| fastembed-rs (ONNX) | ✘ its runtime download is blocked; heavy |
| candle transformer | Possible later as a reranker; slower; weights vendored |
| Remote embedding API | ✘ blocked here; needs a secret; an optional backend elsewhere |

**Vector database: not needed.** The whole corpus is ~15k code items + 20–40k document sections.
At 256 dimensions a brute-force cosine scan over 40k vectors is a few milliseconds and ~10–40 MB of
memory, and `liquers-records` already has a vector column (`Column::Vector`). An HNSW index
(`hnsw_rs`, `instant-distance`) is the step if a memory passes ~100k chunks; external engines
(Qdrant, LanceDB) are what `external-index-sync` is for, not the MVP.

**Embeddings as assets.** An embedding is a derived value keyed by `(model id, chunk text hash)`;
as an asset it is cached and invalidated like any other — no separate index to keep in sync, which
is the property OpenViking needs a refresh queue for.

## 6. Efficiency in the Claude Code cloud environment

The constraints of this container: 4 cores, ~15 GB RAM, a 30 GB disk allowance, a fresh clone per
session, network through a policy proxy.

| Concern | Risk | Proposal |
|---|---|---|
| **Building the binary** | A cold `cargo test -p liquers-lib` is ~3 min and a debug `target/` ~4 GB; the server must not add a second full build | `liquers-mem` built with `liquers-lib` at `default-features = false, features = ["mem"]` (records, no polars / egui / image): a fraction of the full build. Built once per session by a **SessionStart hook in the background**, so the MCP server is ready when the agent first needs it |
| **Starting the MCP server** | Each stdio start builds an `Environment` and registry | Milliseconds once built; declared in the repository's `.mcp.json` so every session gets it with no setup |
| **Index state in an ephemeral container** | Derived assets (catalogue, field records, embeddings) vanish with the container | Recompute is cheap by design: `syn` ≈ 1 s, a BM25 scan reads ~12 MB, static embeddings ≈ 5–10 s. Persist only what is expensive: embeddings in a content-hash-keyed cache file — on a durable mount (F7) when available, never committed to `main` (churn) |
| **Model weights** | Hugging Face is blocked | Vendored: a small crate on crates.io (reachable) embedding the quantized weights, or Git LFS, or a committed file — decided in the semantic-search design |
| **Durable memory** | S3 needs the endpoint allowed by the environment's network policy and credentials in its secrets | Configuration only; the default is a committed `notes/` folder, S3 when the environment allows it |
| **Expensive analyses** | SCIP (169 s / 7 GB) would dominate a session | Produced in CI, fetched when available; never on the session path |
| **Disk** | A second `target/` for the binary | Same workspace `target/`; the reduced feature set shares most artifacts with the normal loop |

The design rule that falls out: **everything on the session path must be cheap to recompute from
the checkout, and anything expensive must be optional and cached by content hash.**

## 7. What Phase 1 takes from this

Adopted in [Phase 1](./phase1-high-level-design.md) (revision 3):

- The purpose is a **competitive MVP**, configurable by profile; AC-18 (the "liquers-specs" profile
  over the unchanged `specs/`), AC-19 (an OpenViking-shaped profile from configuration alone) and
  AC-20 (a remote mount) are added.
- Search is merged; the blocking sequencing question is resolved. Argument overrides replace query
  escaping in the MCP adapter.
- **Context packs** (F2) become part of the MVP: a tenth tool, `memory_context`, with `expand` and
  the record-to-Markdown converters (AC-21).
- **Rust item records** (F4, F5) are proposed for the MVP as an open question: high value, ~1 s per
  session, no new dependency — but they widen the MVP.
- Semantic search (F8), git awareness (F10), SCIP (F11), concept pages (F12) and model-backed
  commands (F13) stay NEXT; a vector database (F14) is not needed.

Filed while writing this: `SIMPLE-TEMPLATE-HAS-NO-COMMAND` and
`VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT`.

## Sources

[model2vec-rs](https://github.com/MinishLab/model2vec-rs) ·
[potion-code-16M](https://huggingface.co/minishlab/potion-code-16M-v2) ·
[Semble](https://github.com/MinishLab/semble) · [fastembed-rs](https://github.com/Anush008/fastembed-rs) ·
[Serena](https://github.com/oraios/serena) · [minijinja](https://github.com/mitsuhiko/minijinja) ·
[rustdoc JSON RFC 2963](https://rust-lang.github.io/rfcs/2963-rustdoc-json.html). Measurements
(network reachability, SCIP time and memory, corpus sizes) were taken in this repository's cloud
container on 2026-10-12. The ranees `expand` and `mustache` commands are in
`orest-d/ranees-website`, `ranees-liquers/src/commands.rs`.
