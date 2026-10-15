---
title: Liquers Vision and Strategy
kind: reference
audience: internal
area: [docs]
reviewed: 2026-10-15
---
# Liquers Vision and Strategy

This document states **why Liquers exists, who it is for, where it is going, and how the workspace
is organized to get there**. It is the background a design or review agent needs before judging
whether a proposal fits the project; [`PROJECT_OVERVIEW.md`](PROJECT_OVERVIEW.md) is its
architectural companion — *what the system is* rather than *why and where to*. Unlike other
`reference/` documents, most of what this one records is intent. Intent is still a claim that can
go stale: when the direction changes, this file changes, on the same review cadence (§9 of
[`DOCS_STRUCTURE_GUIDE.md`](../DOCS_STRUCTURE_GUIDE.md)).

## Purpose

Liquers makes derived data **addressable**: every transformation result has a short,
URL-compatible name — a *query* — that completely describes how to produce it.

```
-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/sort-month/report.html
```

One string is at once a **recipe** (fetch this, apply these commands, serialize to this format), a
**link** that can be bookmarked, shared or embedded in a page, a **cache key** for the asset it
produces, and a **provenance record** of how that asset came to be. The framework parses the
string, plans it against a registry of typed commands, evaluates it asynchronously, tracks its
dependencies, caches the result as an asset in a key-addressed store, and serves it — over HTTP,
in the browser, from Python, or inside a desktop application.

The animating idea: the gap between "I computed this" and "others can get this" should be zero.
A transformation that works in an analyst's session is, with no extra packaging step, a stable URL
in a web service, a cell in a report, and a cached, dependency-audited asset.

## What Liquers is

Liquers is a **framework, not an application**: you supply the commands (typed functions) and the
stores; Liquers supplies the query language, the planner, the asynchronous evaluator, the asset
manager with caching and dependency tracking, serialization, and the serving surfaces. It is
deliberately universal across application shapes — the same command library, unchanged, can be
deployed as:

| Shape | Built from | What it looks like |
|---|---|---|
| **Server / data service** | `liquers-core` + `liquers-axum` (+ `liquers-lib`) | A REST service where every reachable query is an endpoint, with store CRUD and asset APIs included |
| **Client/server** | `liquers-axum` server + `liquers-web` client | The server holds the store and heavy commands; the browser evaluates its own queries and adds JavaScript commands — one language, computation on whichever side it belongs |
| **Browser-only** | `liquers-web` (wasm32) | A static page with no backend at all: environment, evaluation and commands run in the tab |
| **Desktop application** | `liquers-lib` (`egui`) | An interactive data application where every view is a query — shareable, restorable, cacheable |
| **Embedded library** | `liquers-core`/`liquers-lib`, `liquers-py` | Inside another Rust application, or a Python process via bindings |

## Problems it solves

1. **The last mile of data work.** A computed result normally needs an app, an API or a pipeline
   built around it before anyone else can use it. In Liquers the transformation *is* the URL — no
   packaging step between "it works" and "it is served".
2. **Glue-code proliferation.** N transformations × M output formats × K delivery channels
   normally means hand-written handlers, serializers and caches for each. Here they compose
   combinatorially from one registered command library: `eq-…/sort-…/report.html` is not an
   endpoint anyone wrote.
3. **Cache invalidation and freshness.** Derived files silently go stale. Assets carry
   dependencies, versions and expiration; they are reused while valid and recomputed when not —
   *make*, for data, with configurable consistency policies.
4. **Reproducibility and provenance.** "Which script produced this file, from what inputs?" The
   query answers it by construction; metadata and dependency records keep the audit trail.
5. **Platform duplication.** The same logic otherwise gets rewritten for the server, the browser,
   the desktop and Python. One core, compiled everywhere it runs, bound where it does not.

## Where it shines — and where it does not

**Shines:**

- Many small-to-medium transformations over shared data, consumed by many people and views —
  internal data services, reporting, dashboards.
- Wherever freshness and provenance matter more than raw throughput: derived data that must be
  current, explainable and recomputable.
- Mixed-skill teams: developers write and register commands; analysts, front-ends and agents
  compose them into queries without touching the code.
- Low-ops settings: a single-binary server, or no server at all (a static page with wasm).
- Demo-to-production continuity: the query from the prototype is the query in production.
- As an agent tool surface: calls that are validatable offline, cached, and auditable.

**Not the right tool (today, and mostly by design):**

- Distributed big-data batch processing — Liquers schedules assets inside one environment; it is
  not Spark.
- Transactional/OLTP applications — the model is derived, cacheable data, not concurrent mutation.
- Hard-real-time or sub-millisecond hot paths — evaluation is planned and metadata-tracked.
- Continuous stream processing — record *streams* exist, but as re-openable chunked sources, not
  an unbounded streaming engine.

How these answers are told to each audience — pitches, demos, objections — is in
[`POSITIONING.md`](POSITIONING.md).

## The agent workbench

A product direction, not yet a shipped product: Liquers as **the layer where AI agents do
deterministic work** — distinct from the orchestration layer (LangChain/LangGraph, agent SDKs),
which manages the reasoning loop, and anchored to **MCP**, the protocol layer agents converge on.
In one line: *MCP gives agents hands; Liquers gives them a workbench and a lab notebook.*

**The missing middle.** Today an agent's tools come in two flavors: a fixed list of tool
definitions (safe and auditable, but rigid — the fifty-first request needs a developer) or
arbitrary code in a sandbox (infinitely flexible, but unauditable, uncacheable, and unreviewable).
Liquers is the middle: one tool — *evaluate this query* — over a typed command registry. The agent
composes freely, yet every call is **validatable before execution** (`liquers-validate`, no side
effects), **cached** (same query, same asset — across sessions and across agents), and **auditable
after the fact** (provenance, dependencies, metadata). Flexible like code, governed like an API —
and one tool schema in the context window instead of fifty.

**Toolsmithing — agents coin verbs, people govern the vocabulary.** Once Python command
registration lands in `liquers-py`, an agent can *propose* a new command in Python when the
registry lacks a verb it needs. The proposal is ordinary code a human can read, landing in a
staging namespace or realm with provenance (who proposed it, when, from which session), and a
person promotes it into the governed registry. Agent flexibility, human-controlled blast radius.

**Inspectable work.** Queries are short strings a person can read, and they are planned to have a
**visual representation** (a prototype exists in a separate repository): an agent-proposed query
can be rendered, inspected and *modified* by a human in a UI before or after it runs. Oversight of
agent work becomes looking at a diagram, not reading a transcript.

**The ladder.** The agent-memory service is the first application — memory is the asset store
wearing a use case — and its design keeps the general surface separable
(`design/agent-memory-mvp/interfaces-and-mcp.md` §6: a generic MCP engine, tool sets as data). The
steps beyond the MVP are collected in
[`design/agent-memory-mvp/workbench-direction.md`](../design/agent-memory-mvp/workbench-direction.md).

**The hedge, stated plainly.** The workbench is Liquers' wedge and best distribution channel, not
a replacement identity. The durable identity remains the addressable computation substrate above —
which serves humans whether or not the agent ecosystem reshapes. Liquers complements orchestration
frameworks; it does not compete with them, and it is not marketed as one.

## Who it is for, and what it is used for

- **Data services.** Register a command library, mount `liquers-axum`, and every reachable
  query is a REST endpoint. Reports, charts, extracts and dashboards become links instead of
  bespoke handlers.
- **Reproducible pipelines — "make for data".** `recipes.yaml` declares what a store folder
  should contain; assets are computed on demand, reused while their dependencies hold, and
  expired and recomputed when they do not.
- **Interactive exploration and UI.** The same queries drive the egui desktop UI, the
  framework-independent web UI, and the browser, where `liquers-web` lets JavaScript commands
  compose with Rust ones in a single query.
- **Python data science.** Liquers began as the Python [LiQuer](https://orest-d.github.io/liquer/site/index.html)
  framework; `liquers-py` keeps Python a first-class consumer of the Rust core.
- **Agent and LLM infrastructure.** Queries are cheap, auditable, cache-friendly tool calls:
  a plan can be validated offline before anything runs, and results carry metadata and
  provenance. The agent-memory service (`specs/design/agent-memory-mvp/`) is the first design
  deliberately aimed at this audience; the larger direction is §The agent workbench below.

## Vision

1. **Everything is an asset.** Stored files and computed results share one hierarchical key
   space. Asking for data never requires knowing whether it already exists — the asset manager
   finds it, reuses it, or computes it, and its metadata records which.
2. **One query language everywhere.** The same query means the same thing on a Rust server, in
   a browser tab, from Python, and in a desktop app. Hosts differ in which commands their
   *realm* provides, never in what the language means.
3. **Trust the result.** Metadata, dependency records, versions and expiration make an asset
   auditable: what produced it, from what, and whether that is still current.
4. **Useful out of the box.** A rich standard library — tabular records, Polars DataFrames,
   images, UI elements — so that real work starts at the query, not at plumbing.
5. **Supersede the Python implementation.** The Rust workspace is a complete redesign of
   LiQuer, intended to replace it while keeping Python users on board through bindings.

## Strategy

How the workspace is organized to deliver that vision:

- **Minimal core, rich layers.** `liquers-core` defines the language and the traits
  (`ValueInterface`, `AsyncStore`, `CommandExecutor`, `Environment`) with few dependencies;
  everything heavy sits above it and is feature-gated (`polars`, `egui`, `image-support`,
  `records`), so minimal and wasm builds stay viable.
- **Async-first, wasm as a forcing function.** There is one execution model — async — and the
  browser target keeps it honest: conditional `Send`/`Sync` markers and the spawn-free
  `ImmediateAssetManager` exist so the same core runs on `wasm32`. Synchronous surfaces exist
  only as convenience wrappers for bindings; the synchronous store trait was removed and does
  not come back.
- **Extensibility through traits and declarative registration.** New value types, stores and
  commands are added by implementing traits and registering through `register_command!` and the
  store factories — not by modifying core. Realms and namespaces let different hosts (server,
  browser, desktop) carry different command sets without forking the language.
- **One host per integration crate.** Each boundary gets its own crate (`liquers-axum`,
  `liquers-web`, `liquers-py`) that adapts rather than extends the model, following
  [`guides/LANGUAGE-INTEGRATION_GUIDE.md`](../guides/LANGUAGE-INTEGRATION_GUIDE.md).
- **Specification-driven development.** Designs go through the phased workflow under
  `specs/design/`, issues are filed before they are forgotten, and `reference/` is kept true at
  HEAD — the documentation system is built so that autonomous agents can design, implement and
  review safely. This is itself strategic: Liquers is developed largely *by* the kind of agent
  it wants to serve.

### Crate roles

| Crate | Purpose | Strategic role |
|---|---|---|
| `liquers-core` | Query language, plans, values, assets, stores, errors | The contract everything else implements; kept minimal and dependency-light |
| `liquers-macro` | `register_command!`, `command_version` | Makes command registration declarative and cheap, so libraries grow |
| `liquers-store` | OpenDAL-backed store backends | One integration reaching many storage services (S3, WebDAV, …) instead of one backend per service |
| `liquers-records` | Columnar record streams, table formats, manifests | Tabular data without the weight of a DataFrame engine; depends on core only, so anything can build on it |
| `liquers-lib` | Extended values (DataFrames, images, UI, records glue) and the command library | The batteries — where "useful out of the box" lives |
| `liquers-axum` | HTTP REST API builders | Queries as web services; uses core + store only, so servers need not pull in the rich library |
| `liquers-web` | Browser/JavaScript bindings (wasm32) | The language in the browser; JavaScript commands as first-class citizens |
| `liquers-py` | Python bindings (PyO3) | Continuity with LiQuer's user base; Python as consumer, not second implementation |

### Priority areas

1. **The agent workbench** (§above) — the agent-memory MVP and its MCP surface, with **Python
   command registration** in `liquers-py` and **wasm support** as its enablers; the current top
   priority.
2. **DataFrames and records** — the highest-priority data domain (Polars in `liquers-lib`,
   record streams in `liquers-records`).
3. **More storage backends** — databases and cloud services, through the store factory model.
4. **Better Python integration** — tighter data-science ecosystem fit (and the toolsmithing
   prerequisite above).
5. **Web UI / dashboards** — an interactive interface for queries.
6. **Extended value library** — images, matrices/tensors, ML models.

### UI roadmap

1. Desktop egui application (reference backend).
2. WASM egui (the desktop UI in the browser).
3. Framework-independent HTML backend (`webui`; see `specs/design/webui/`).
4. **Visual query representation** (prototype in a separate repository): render a query as an
   editable structure, so queries — agent-proposed ones included — can be inspected and modified
   in a UI rather than as strings.
5. Also planned: a terminal UI.

## Non-goals

- **No stateful query language.** Queries and recipes describe stateless executions
  reproducible from the store and the command registry; mutable session state does not enter
  the language. (Commands may touch non-static systems; dependency checking mitigates, not
  licenses, that.)
- **No synchronous store.** Removed deliberately (`design/sync-store-removal/`); bindings wrap
  async instead.
- **Core does not grow rich features.** A capability that can live in `liquers-lib` or an
  integration crate does.
- **Not a distributed-workflow orchestrator.** Liquers schedules asset evaluation inside one
  environment; cluster orchestration, if ever, is a layer above it, not a core concern.

## Status

The workspace is at version `0.1.0` and the public API is evolving. Where this document's intent
and the code disagree, the code and its tests are authoritative about the present — this document
is authoritative only about direction. Current maturity per capability:
[`specs/README.md`](../README.md).

## How design and review agents should use this

- **Phase 1 (high-level design):** state the purpose in terms of this document — which use case
  or vision point the design serves. A design serving none of them needs an explicit case for
  extending the direction, made to a human, not decided silently.
- **Review:** check crate placement against the crate-roles table and the dependency flow in
  `CLAUDE.md`; check that the design does not cross a non-goal. A conflict with this document is
  a finding to raise, not a reason to rewrite the document mid-review.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-15 | Added §The agent workbench (the missing middle, toolsmithing, inspectable queries, the hedge); the workbench leads the priority areas with Python command registration and wasm as enablers; visual query representation added to the UI roadmap. | user direction; `design/agent-memory-mvp/workbench-direction.md` |
| 2026-10-15 | Added §What Liquers is (application shapes), §Problems it solves, §Where it shines — and where it does not; linked `POSITIONING.md`. | user request |
| 2026-10-15 | Created: purpose, uses, vision, strategy, crate roles, non-goals. Priority areas and UI roadmap moved here from `PROJECT_OVERVIEW.md` §Future Plans. | documentation-gap review |
