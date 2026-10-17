---
title: Positioning and Marketing Plan (draft)
kind: reference
audience: internal
area: [docs]
reviewed: 2026-10-15
---
# Positioning and Marketing Plan — draft

**Status: draft under discussion.** This is the first draft of how Liquers is described to people
who have never heard of it, per audience, plus a rollout sketch. The facts it leans on are in
[`VISION.md`](VISION.md) (purpose, application shapes, problems solved, where it shines); this
document is about *telling* them. The open questions at the end are real decisions, not filler —
nothing below is final until they are answered.

**Honesty rule.** Every pitch distinguishes what is demonstrable today from what is designed but
not shipped. The workspace is at 0.1.0 with an evolving API; marketing that hides this converts
early adopters into ex-users. "Upcoming" below always means: a design exists under `specs/design/`,
the code does not ship yet.

## Positioning statement

> For teams that turn stored data into views, reports and services, **Liquers** is a query-driven
> computation framework that makes every transformation an **addressable, cacheable, auditable
> asset** — a short URL-compatible string that is at once the recipe, the link, the cache key and
> the provenance record. Unlike bespoke API endpoints, notebooks or workflow engines, the same
> query means the same thing on a Rust server, in the browser, on the desktop and from Python.

### One-liners (candidates — pick one, see open question 3)

1. *The URL is the pipeline.*
2. *Every piece of derived data, one link away.*
3. *Make, for data — with URLs.*
4. *Compute it once, link it forever.*
5. For the agent audience specifically: *Agents think in models; they work in Liquers.*

### The 90-second explanation (for someone with no context)

You know how a spreadsheet cell recalculates when its inputs change, and you can point at it?
Liquers does that for whole data transformations. You write a short path-like string:

```
-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-month/report.html
```

— "take this file, read it as records, keep the EU rows, sort by month, render it as an HTML
table". That string is a link you can open in
a browser, a recipe anyone can rerun, and a cache key: Liquers computes it once, remembers what it
depended on, and recomputes it only when that changes. Developers add the verbs (commands) in Rust,
JavaScript or (via bindings) Python; everyone else — colleagues, web pages, AI agents — composes
them. The same engine runs as a web server, inside a browser tab with no backend, in a desktop
app, or embedded in Python.

## Message pillars

| # | Pillar | Claim | Proof |
|---|---|---|---|
| P1 | **Addressable** | The URL is the pipeline: recipe = link = cache key = provenance | Any query demo; `liquers-validate` showing the plan |
| P2 | **Trustworthy** | Assets carry dependencies, versions, expiration; stale results recompute themselves | `recipes.yaml` folder demo; `DEPENDENCIES_STATUS.md` |
| P3 | **Everywhere** | One language: server, browser, desktop, Python | axum quickstart + the same query in the `liquers-web` static page |
| P4 | **Batteries included** | Records, Polars DataFrames, images, UI elements out of the box | `ns-pl` command library; command registry |
| P5 | **Composable by non-authors** | Developers write commands; analysts, front-ends and agents combine them | A query nobody wrote an endpoint for |

## Audience pitches

Each pitch: the hook (their pain, their words) → the pitch → the killer demo → the likely
objections with honest answers. Every audience page or talk should lead with the hook, not with
architecture.

### 1. Backend developer

- **Hook:** "Every new report, extract or chart means another endpoint: route, parameter parsing,
  serialization, caching, invalidation. Again."
- **Pitch:** Register typed commands once; `liquers-axum` exposes a composable query API over all
  of them — `GET /q/{query}` — with caching, metadata and dependency-driven invalidation built in.
  The server crate depends only on `liquers-core` + `liquers-store`, so a lean service stays lean.
  Queries are not stringly-typed chaos: plans are built against a typed command registry, surplus
  parameters are positioned errors, and `liquers-validate` checks a query offline before anything
  runs.
- **Killer demo:** ~30 lines of Rust → a running service where
  `/q/-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/report.html` works, then a second query
  nobody wrote a handler for.
- **Objections:**
  - *"Another DSL to learn?"* — It is URL-shaped and sparse by design; most pipelines are
    one-liners, and the validator explains any query's plan.
  - *"Arbitrary queries from the internet against my data?"* — Only registered commands run, and
    realms/namespaces bound what a host exposes. Honest caveat: per-key/per-session access control
    is an open issue (`CORE-SESSION-AND-KEY-ACL`); today you gate at the HTTP layer.
  - *"AGPL-3?"* — The real blocker for some shops; see open question 1.

### 2. Front-end developer

- **Hook:** "I'm blocked on the backend team for every data shaping change. And my demo needs a
  server I don't have."
- **Pitch:** `liquers-web` runs the whole engine in the browser as WebAssembly. A static HTML page
  constructs an environment, evaluates queries as Promises, and registers commands **in
  JavaScript** that compose with the built-in Rust ones in a single query (`myCommand/to_text`).
  No backend, no build step beyond the wasm bundle. When a backend exists, the same queries move
  server-side unchanged.
- **Killer demo:** one static page where editing a query string in an input re-renders the view —
  openable from a file share, deployable on any static host.
- **Objections:** *wasm bundle size* (measure and publish it); *SSR/SEO* — a server-side HTML
  rendering mode is designed (`specs/design/webui/`), upcoming.

### 3. Application developer (desktop and full applications)

- **Hook:** "Application state is a pile of ad-hoc structs; 'send me what you're looking at' is a
  screenshot."
- **Pitch:** Build the application so every view is a query. State becomes a string: shareable,
  bookmarkable, restorable, cacheable; navigation history is a history of queries. The egui
  backend ships today for desktop; a framework-independent web backend (`webui`) is designed, so
  the same application logic targets desktop and browser.
- **Killer demo:** the egui data browser — paste a query, watch the asset lifecycle (progress →
  value) render live.
- **Objections:** *UI maturity* — honest status: egui is the reference backend, `webui` is phase-1
  designed; the UI roadmap is in `VISION.md`.

### 4. Data scientist

- **Hook:** "My notebook works. Productionizing it is a different project. And nobody knows which
  script produced `final_v3_REAL.csv`."
- **Pitch:** Your pipeline is a string — paste it in chat, put it in a README, bookmark it.
  Polars-backed commands (`ns-pl`) cover the DataFrame verbs; a `recipes.yaml` turns a folder into
  a self-materializing target set — *make for data*: assets recompute when sources change and are
  reused when they have not. `liquers-py` keeps you in Python; Liquers began as the Python LiQuer
  framework, so this audience is its oldest.
- **Killer demo:** a folder with `recipes.yaml`; touch the source CSV, re-request `report.html`,
  watch it recompute — and only it.
- **Objections:** *Python coverage* — honest: `liquers-py` exposes a selected subset of core
  today; the strategy (deeper data-science integration) is priority 3 in `VISION.md`.

### 5. User of AI agents — **upcoming** (agent memory system, then the agent workbench)

- **Hook:** "My agent re-derived the same analysis for the ninth time at $4 a run, its memory is a
  pile of Markdown files or a vendor-locked vector store, and I can't explain to anyone what it
  actually did."
- **Category:** the **agent workbench** (`VISION.md` §The agent workbench). Orchestration
  frameworks (LangChain/LangGraph, agent SDKs) manage the reasoning loop; Liquers is the layer
  where the agent's *work* lives. One line: *MCP gives agents hands; Liquers gives them a
  workbench and a lab notebook.* The core argument is **the missing middle**: fixed tool lists are
  safe but rigid, arbitrary code in a sandbox is flexible but unauditable — a Liquers query is
  compositional like code and validatable *before* execution, cached across sessions and agents,
  provenance-tracked after.
- **Pitch, step 1 (memory, designed: `specs/design/agent-memory-mvp/`):** a memory agents browse
  in tiers, search, write safely and draw assembled context packs from — served over **MCP** and a
  CLI, configured by one document, running over *your* stores: local directories or remote ones
  such as S3. Not a black box: the derived indexes are assets you can inspect by URL.
- **Pitch, step 2 (workbench, planned: `design/agent-memory-mvp/workbench-direction.md`):**
  evaluate/validate/discover as a general tool set; **toolsmithing** — the agent *proposes* a new
  command in Python when the registry lacks a verb, and a person promotes it into the governed
  registry (*agents coin verbs; people govern the vocabulary*); a **query inspection UI** that
  renders an agent-proposed query as an editable structure, so oversight is looking at a diagram,
  not reading a transcript.
- **Killer demo (once shipped):** `memory_new(...)` returning ranked duplicate candidates before
  writing anything — one MCP call replacing a grep-open-judge-scaffold loop. Workbench follow-up:
  the same query, issued by two different agents a week apart, computed once.
- **Objections:** *"It doesn't exist yet"* — correct; market each step as early access when it
  lands, not before. *"Why not a vector DB?"* — tiers, provenance and configurability over raw
  similarity; the design's survey (`memory-systems-survey.md`) makes the comparison honestly.
  *"Why not a code sandbox?"* — a sandbox runs anything once; the workbench runs governed verbs
  with a cache and a receipt.

### 6. Documentation writer — **upcoming** (template system)

- **Hook:** "Every number, table and example in our docs was true the day it was written."
- **Pitch:** Liquers templates embed queries in documents — Mustache-style tags whose contents go
  straight to the query parser (language evaluated and recommended in
  `agent-memory-mvp/template-language-evaluation.md`). Rendering goes through the asset layer, so
  a document *declares* its data: embedded queries are cached, run concurrently, and recorded as
  dependencies — the rendered page is invalidated when its data changes. A malformed query inside
  a template is a parse error at line:column **before anything runs**. Today: `simple_template`
  ships in core (`Result: $load/to_text$`), and this repository's own generated index blocks are
  the working precedent.
- **Killer demo (once shipped):** a status report whose table re-renders when the underlying
  records change — and a CI check that fails on a template whose query no longer parses.
- **Objections:** *"We have Jinja/MkDocs macros"* — those see a query as an opaque string, cannot
  position its errors, and render outside the asset layer; no caching, no invalidation, no
  provenance.

## Proof points: today vs upcoming

| Claim | Today | Upcoming (designed) |
|---|---|---|
| REST data service | `liquers-axum`, query + store + assets APIs | some asset mutation routes still unimplemented |
| Browser, no backend | `liquers-web` quickstart, JS commands | SSR via `webui` |
| Desktop UI | egui backend, asset lifecycle views | `webui` HTML backend, TUI |
| DataFrames | Polars `ns-pl` library | — |
| Records/tables without Polars | `liquers-records`, CSV/NDJSON/Markdown/HTML/IPC/Parquet | — |
| Make-for-data | recipes, dependency audit, expiration, consistency policies | asset GC, combined expiration algebra |
| Python | `liquers-py` subset | deeper data-science integration |
| Agent memory | design complete (MVP, rev 3) | implementation |
| Agent workbench | separability decided (`interfaces-and-mcp.md` §6); `liquers-validate` | general tool set, Python command registration, toolsmithing, query inspection UI (`workbench-direction.md`) |
| Templates | `simple_template` in core | Mustache-with-query-tags engine |
| Offline validation | `liquers-validate` CLI + registry export | editor/live validation (`UI-QUERY-EDITOR-LACKS-LIVE-VALIDATION`) |

## Rollout sketch (draft)

- **Stage 0 — groundwork.** Rewrite the root README's opening into the 90-second explanation with
  one runnable snippet; one 5-minute quickstart per persona; publish the **browser-only demo
  page** — a static page is the cheapest possible "wow": zero install, shareable as a link, and it
  demonstrates P1+P3 by existing.
- **Stage 1 — beachhead.** Concentrate on one persona first (open question 2). Recommendation:
  the **backend developer / data scientist pair** around `liquers-axum` + Polars + recipes,
  because that path is the most complete today and its demo is the most production-shaped.
- **Stage 2 — content.** Blog posts matching the pillars (*The URL is the pipeline*; *Make, for
  data*; *One query language, four runtimes*), Show HN / r/rust / PyData, crates.io metadata and
  keywords, honest comparison pages (vs hand-rolled REST+cache, vs notebooks, vs dbt/DVC, vs
  workflow engines).
- **Stage 3 — the agent moment.** The MCP memory service launch is the highest-attention channel
  available to a project this size in 2026; time a dedicated push (MCP server listings, agent
  tooling communities) to the MVP actually shipping, with the configuration-document story
  (same service shaped like this repo's `specs/` workflow or like a context database) as the
  differentiator.
- **Measures.** Quickstart completion (can a stranger finish in 5 minutes?), demo-page visits,
  crates.io downloads, GitHub stars/issues from strangers, MCP installs once shipped.

## Open questions (decisions needed)

1. **License.** The workspace is AGPL-3. For the backend-developer and embedded audiences this is
   the first objection, not the last; it effectively forbids many commercial deployments without a
   second license. Is AGPL a deliberate strategy (dual licensing / open-core), or should the
   positioning — or the license — change? The marketing cannot be written around it.
2. **Beachhead.** Which persona first? Draft recommends backend/data-scientist (most complete
   today); the alternative bet is the browser demo (widest reach, weakest production story) or
   waiting to lead with the agent story (highest attention, not yet shipped).
3. **One-liner.** Pick from the candidates (§One-liners) or supply a better one.
4. **Name and searchability.** "Liquers" reads as *liquors/liqueurs* to search engines and humans;
   decide whether that is embraced (memorable, with a tagline that puns deliberately) or mitigated
   (always "Liquers framework" in titles; own the spelling in docs).
5. **Claims discipline at 0.1.0.** What may public material promise about API stability? Draft
   rule: demos pin a version; prose never promises compatibility before 0.2.
6. **Hosted playground.** A public axum instance with a sandboxed store would strengthen every
   pitch but needs the ACL story (`CORE-SESSION-AND-KEY-ACL`) or heavy sandboxing. Worth the
   investment now, or after the beachhead?

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-15 | Agent audience repositioned to the **agent workbench** framing (the missing middle, toolsmithing, query inspection UI); added the agent tagline and a workbench proof-point row. | user direction; `VISION.md` §The agent workbench |
| 2026-10-15 | Created: positioning statement, one-liner candidates, 90-second explanation, message pillars, six audience pitches (backend, front-end, application developer, data scientist, AI-agent user, documentation writer), proof-point table, rollout sketch, open questions. | user request |
