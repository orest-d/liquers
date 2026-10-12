# Agent memory service — capability specification

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-11. It specifies **what the
service must do**, derived from the survey of existing systems
([`memory-systems-survey.md`](./memory-systems-survey.md) §4, services S1–S22), from this
repository's own practice ([`workflow-porting.md`](./workflow-porting.md)), and from the tier
analysis ([`tiers-and-storage.md`](./tiers-and-storage.md)). The delivery surfaces — CLI, HTTP,
MCP — are in [`interfaces-and-mcp.md`](./interfaces-and-mcp.md).

It is a specification of behaviour, not of types: signatures are Phase 2's. Requirement levels are
**MVP** (this design delivers it), **NEXT** (designed for, not built: the MVP must not foreclose it)
and **OUT** (deliberately excluded, with the reason).

## 1. Principles

These are the decisions everything below follows from. Each is a choice between the camps of the
survey (§3 there).

| # | Principle | Chosen over | Why |
|---|---|---|---|
| P1 | **Files are the truth.** A memory is a document in a store — Markdown with front-matter by default — readable and editable without the service | A database of facts (mem0, Zep) | Review. This repository's whole process is that a human or an agent can read, diff and correct what is known. Every file-based system in the survey keeps that property; no database-based one does |
| P2 | **The calling agent does the thinking; the service does the bookkeeping.** The MVP service calls no language model | Service-side extraction (mem0, OpenViking) | No credentials, deterministic tests, and the agent already holds the context the extraction needs. mem0's 2026 retreat from model-driven updates is evidence that service-side consolidation is the riskiest part. A model-backed command is NEXT, and never writes a tier that looks hand-written (`tiers-and-storage.md` §4) |
| P3 | **Derived content is an asset.** Indexes, catalogues, folder overviews and generated tiers are recipes, recomputed when a source's content-hash version changes | Regenerate-and-diff (`docs_index.py`), freshness queues (OpenViking) | It is what Liquers is for, and no surveyed system has general derived-data invalidation |
| P4 | **Supersede, do not destroy.** Edits are recorded; replacing a memory links old to new; forgetting (NEXT) will archive rather than delete, unless asked to purge | In-place mutation (mem0 v2) | Survey §3.3: lossy consolidation is the most consistent failure. This is also `specs/`' own rule (archive, `superseded_by`) |
| P5 | **Search never evaluates.** Reading and searching memory never starts a recipe | Evaluate-on-read | Inherited from `store-and-asset-search` (conclusion 1): one query must not recompute a corpus |
| P6 | **Schema is data in the memory.** Collections, fields, vocabularies, templates and check rules are documents under `_schema/`, not code | Hard-coded rules (`docs_index.py`) | One service serves this repository's `specs/`, an agent's notes and an unrelated project; the LLM-wiki "schema" layer becomes machine-readable |
| P7 | **A small fixed tool surface; the query language carries composition** | One tool per command | `store-and-asset-search/research-questions.md` §6; every surveyed MCP surface is 3–20 tools |
| P8 | **One writer, no identity** in the MVP | Scopes with ACL (OpenViking) | `CORE-SESSION-AND-KEY-ACL` is unresolved and out of scope (`DESIGN.md`); scopes are folders, not principals |

## 2. The memory, as data

### 2.1 Layout

A memory is a key prefix (default `mem/`) assembled by `AsyncStoreRouter` from mounts. The layout
is a **convention declared in `_schema/`**, not wired into code; the default:

```
mem/
  README.md            the memory's own L0/L1 and the "start here" page (core memory)
  _schema/             collection definitions (P6)
  _templates/          templates instantiated by `new`
  _log/                write log, one Markdown file per month (S19; LLM-wiki log.md)
  <collection>/…       documents; a collection is a folder with a schema
```

Keys given to and returned by the tools are **relative to the memory root**: `specs/issues/X.md`,
not `mem/specs/issues/X.md`.

For this repository the collections are the existing genres: `specs/issues`, `specs/design`,
`specs/reference`, `specs/guides` — writable, but **only through the service's write commands**
(which validate, stamp and log) — `specs/archive`, mounted read-only, and `.claude/skills` as
`skills`. Agent-written memory goes to collections in a **separate, writable
mount** — `notes/` (semantic: entities, concepts, decisions), `episodes/` (dated session
summaries), `inbox/` (raw captures awaiting consolidation). Whether agent notes are committed to
git is a deployment choice (Open question 4 in Phase 1).

### 2.2 A memory document

```markdown
---
title: Asset expiration does not cascade through keyed recipes      # L0 (required)
description: >                                                      # L1 (required)
  Expiring a source key leaves keyed dependents Ready; the cascade only follows query
  dependencies. Seen while fixing keyed-expiry; worked around by expiring both.
kind: finding                       # collection-defined vocabulary
created: 2026-10-11                 # stamped by the service
updated: 2026-10-11                 # stamped on every write through the service
source: specs/design/keyed-expiry-cascade-fix/   # provenance (S10), a key or URL
superseded_by:                      # P4: set instead of deleting
expires:                            # optional validity end (S9)
---
Body (L2) …
```

Only `title` and `description` are universal. Everything else is a field of a collection schema.

### 2.3 A collection schema

```yaml
# mem/_schema/issues.yaml — what docs_index.py and DOCS_STRUCTURE_GUIDE.md §4 encode today
collection: issues
folder: specs/issues
filename: "{id}.md"
id: { from: title, case: screaming-kebab, max_len: 60 }
template: _templates/issue.md
fields:
  id:         { required: true, equals: filename_stem }
  kind:       { required: true, enum: [issue, feature] }
  status:     { required: true, enum: [draft, accepted, rejected, duplicate, in_progress,
                closed, closed_not_planned], transitions: _schema/issue-status.yaml }
  priority:   { required: true, enum: [P0, P1, P2, P3] }
  complexity: { required: true, enum: [S, M, L, XL] }
  area:       { required: true, list: true, enum_from: _schema/areas.yaml }
  design:     { link: design, reciprocal: issues }
  created:    { required: true, type: date }
sections: [Problem, Impact, Expected behaviour, Discovery]
checks: [no_dead_links, title_max_len: 120]
```

The schema language is Phase 2's to fix; the requirement is that `docs_index.py --check`'s rules
over `specs/` are expressible in it (AC-9), with any residue written as a named built-in rule.

## 3. Capabilities

### 3.1 Read

| # | Capability | Level | Mechanism |
|---|---|---|---|
| R1 | **Overview** — one call returns the memory's README L0/L1, its collections with their L0/L1 and counts, and the latest log entries: what an agent reads first (Claude Code's `MEMORY.md`, the memory tool's "view your memory directory first") | MVP | Recipe over `README.md`, `_schema/`, `_log/` |
| R2 | **List** a folder with every entry's L0/L1, kind and status, without reading bodies | MVP | Assets API `listdir` + the front-matter store (`tiers-and-storage.md` C) |
| R3 | **Search** — words, phrases, regex, field filters, scoped to a folder, ranked, with `matched`, `excerpt`, and an "N of M" truncation signal | MVP | `ns-search` (`catalog` / `field_extract` / `search`) |
| R4 | **Section-level search** — return the passage, not the file | MVP | `ns-search/md_split` |
| R5 | **Read** at a tier — L0, L1, full; one section by heading; a line range | MVP | `-R/<key>`, metadata, `md_split` / `ns-mem/section` |
| R6 | **Hide superseded** memories from list and search unless asked; hiding **expired** ones is NEXT | MVP / NEXT | A filter the tool adds, not `search`: `-superseded_by:*`, and later `-expires:<` today's date. Not a bare `-expired`, which in the search grammar is a negated *word*. How an empty YAML `superseded_by:` evaluates under `field:*` is checked in Phase 3 |
| R7 | **Links** — outgoing links of a document, documents linking to it | NEXT | `ns-mem/links` (overlaps `NO-MARKDOWN-NAVIGATION-COMMANDS`) |
| R8 | **Semantic search** | NEXT | `external-index-sync` (vector store as a materialized view) |
| R9 | **Recency / importance ranking** | NEXT | `sort` from `record-filter-commands`; a scoring policy is not a search primitive |
| R10 | **Describe** — collections, fields, vocabularies, templates: what an agent may filter on and write | MVP | `ns-mem/describe` over `_schema/` |

### 3.2 Write

| # | Capability | Level | Mechanism |
|---|---|---|---|
| W1 | **Write** a memory: key, body, `title`, `description`, fields. Refuses to create a Markdown memory without L0/L1, an unknown field value, a key outside a writable mount | MVP | `ns-mem/write`. Writes go through the asset manager (under its per-key lock), not straight to the store, so live assets and their dependents see them; write commands are volatile, the body is the payload rather than a query parameter, so an identical repeated write is not answered from cache |
| W2 | **Edit** in place: replace an exact string (the memory tool's `str_replace`), set a front-matter field, append a table row to a named section | MVP | `ns-mem/edit`, `set_field`, `append_row` |
| W3 | **Precondition** — every write may carry the version it was based on, and is refused if the memory changed since | MVP (single process) | Check-then-write in the command; atomic only with `STORE-WRITE-HAS-NO-PRECONDITION` fixed |
| W4 | **New from a collection** — id from the title, template instantiated, fields validated; **duplicate candidates returned first**, created only when none or when forced | MVP | `ns-mem/similar` + `ns-mem/new` |
| W5 | **Supersede** — write a new memory and set `superseded_by` on the old one in one call | MVP | `ns-mem/write` argument |
| W6 | **Forget** — move to an archive collection with a reason (default), or purge on explicit request | NEXT | `ns-mem/forget`, once the archive convention is fixed; in the MVP, supersession (W5) or a status |
| W7 | **Write log** — every service write appends a line to `_log/YYYY-MM.md`: date, operation, key, version before and after, one-line reason | MVP | Inside the write commands; OpenViking's `memory_diff.json` and the LLM-wiki `log.md` |
| W8 | **Stamp** `created` / `updated` on write | MVP | Inside the write commands |
| W9 | **Ingest** a raw source (URL, file) into `inbox/` unchanged, with provenance | NEXT | A command around existing resource loading |
| W10 | **Model-backed extraction / summarisation** (session → memories; document → proposed L0/L1) | NEXT | A feature-gated command, or MCP *sampling* so the client's model does it (`interfaces-and-mcp.md` §5) |
| W11 | **Git commit** of service writes | OUT for MVP | The agent or human commits, as now; Letta's commit-every-edit is a deployment option later |

### 3.3 Maintain

| # | Capability | Level | Mechanism |
|---|---|---|---|
| M1 | **Check** — schema rules, required sections, dead links, reciprocal links, L0/L1 present and within length, `reviewed:` age; results as records `(rule, key, severity, message)` | MVP | `ns-mem/check` |
| M2 | **Derived indexes** — `specs/index.csv` and `specs/index.md` as recipes (MVP); README generated blocks (NEXT) | MVP | Recipes over separate simple commands: `field_extract`, `ns-rec` filters and `sort`, `count_by`, `to_md_table` / `to_csv`, and a template for the page (Phase 1 decision 10). Byte equality with `docs_index.py` also needs its computed columns (`status_source`; `readiness`/`autofix` joined across issues and designs; one design row per `DESIGN.md`), each a command, not a projection |
| M3 | **Generated tiers** for documents and folders without front-matter (deterministic rule) | MVP | First heading / first paragraph, or a section the collection names; folder L0/L1 from `README.md`. This is where `specs/` gets its L1 today: no document there has `description:` |
| M4 | **Consolidation candidates** — near-duplicates and unlinked related memories, for the agent to merge | NEXT | `similar` over a collection; the merge is judgement |
| M5 | **Staleness report** — superseded-but-referenced, expired, overdue `reviewed:` | MVP (as `check` rules) | M1 |
| M6 | **Contradiction detection** | OUT | Needs a model and is unreliable; LLM-wiki lint leaves it to the agent too |

### 3.4 Operate

| # | Capability | Level |
|---|---|---|
| O1 | Run embedded in one process: CLI and stdio MCP open the stores directly, no server needed | MVP |
| O2 | Run as a server: the assets API plus MCP over Streamable HTTP in one `liquers-axum` router | NEXT (Phase 1 question 5; `interfaces-and-mcp.md` §7) |
| O3 | Read-only corpus mount that never writes a sidecar | MVP prerequisite (`STORE-NO-READ-ONLY-ADAPTER`) |
| O4 | Scopes per user / agent / session, ACL | OUT (P8) |
| O5 | Retrieval trace (why these hits, which scope, how many read) | NEXT; MVP returns `matched`, `excerpt`, totals |

## 4. Why the service does not call a model (P2), in more detail

The survey splits evenly. The arguments specific to Liquers:

- **Determinism is how this repository works.** Every design here proves itself with tests and an
  oracle (`docs_index.py`). A model in the write path makes the oracle impossible.
- **The client already is a model.** An agent connected over MCP can write the L0/L1 as it writes
  the memory (W1 requires them). Where the server does want a model's help, MCP *sampling* lets the
  server ask the *client's* model, with no credentials on the server.
- **Liquers commands are the right place for it later.** A `summarize` command is an ordinary
  command; as a recipe its output is cached and invalidated like any asset. Nothing in the MVP
  forecloses it, which is why W10 is NEXT rather than OUT.

## 5. Prerequisites and related work

| Item | State at `main` `df2d5ee` | Effect here |
|---|---|---|
| `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` | **Closed** (design `axum-assets-endpoints`, PR 73) | The blocker named by the first Phase 1 is gone; `key/listdir` and `key/info` exist |
| `store-and-asset-search` | Merged 2026-10-12 (complete) | R3, R4, M1's field records and W4's similarity all build on `ns-search`; **this design follows it** |
| `STORE-NO-READ-ONLY-ADAPTER` | Draft, P2 | O3 needs it; must be fixed first or worked around by a read-only wrapper inside this design |
| `STORE-WRITE-HAS-NO-PRECONDITION` | Draft, P2 | W3 is racy across processes until fixed; acceptable under P8 |
| `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` | Draft, P2 | Fields other than L0/L1 reach clients as records, not metadata; not blocking |
| `NO-MARKDOWN-NAVIGATION-COMMANDS` | Draft, P3 | R5's section read, R7; this design may absorb the minimum it needs |
| `external-index-sync` | Phase 1 not started | R8 |
| `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO` | Draft, P2 (filed 2026-10-11) | Folder tiers through a router member; worked around by wrapping the router |
| `ARGUMENT-INFO-HAS-NO-DESCRIPTION` | Draft, P3 | MCP argument descriptions cannot all come from `CommandMetadata` until fixed; tool declarations supply them meanwhile |
| `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT` | Draft, P2 | Needed before a general `query` tool may be offered over MCP to an untrusted client |
