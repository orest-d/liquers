# Agent memory systems — survey

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-11. It records what the
established agent memory systems do, so that the service specification
([`service-specification.md`](./service-specification.md)) is measured against practice rather than
against one system. Each system is described by data model, write path, read path, interfaces and
failure modes; §4 extracts the services they have in common.

**Evidence.** Facts were read in primary sources (repository READMEs, docs in the repositories,
source files) on 2026-10-11 unless marked *(unverified)*: then they come from secondary sources or
from earlier knowledge, because the primary source was unreachable. Systems change quickly — two of
the headline facts below are reversals made in 2026 — so re-check before relying on a detail.

## 1. The three systems the brief names

### 1.1 OpenViking (volcengine/OpenViking, v0.3.22)

Sources: <https://github.com/volcengine/OpenViking>, `docs/en/concepts/02-context-types.md` …
`08-session.md`, `docs/en/api/16-memory.md`, `docs/en/guides/06-mcp-integration.md`.

- **Model.** A virtual filesystem under `viking://` URIs with three context types: *resources*
  (documents, repositories, web pages), *memories* (extracted from sessions or written explicitly)
  and *skills* (`SKILL.md` + scripts). Namespaces: `viking://resources/` (account-wide),
  `viking://user/{id}/` (`memories/`, `resources/`, `skills/`, `peers/{peer}/`, `sessions/{id}/`),
  `viking://agent/skills/`. Memory categories are files and folders: `profile.md`, `preferences/`,
  `entities/`, `events/`, `identity.md`, `soul.md`, and the "evolution" set `cases/`,
  `trajectories/`, `experiences/`; custom categories come from templates.
- **Tiers.** L0 *abstract* (`.abstract.md`, ≤ 256 characters by default), L1 *overview*
  (`.overview.md`, ≤ 4,000 characters), L2 the original files. **L0/L1 are per-directory sidecars**,
  generated bottom-up by a language model: per-file summaries, then the folder's L1, then L0 taken
  from L1's first paragraph. Freshness propagates upward: an unchanged child does not refresh its
  parent; large folders refresh when 10 % of their children changed.
- **Write path.** Resources: parse (Markdown split at ~2k tokens, PDF, HTML, code skeletons via
  tree-sitter, media) → semantic queue → embedding queue. Memories: a session collects messages;
  `commit()` archives them synchronously and extracts memories in the background, informed by the
  existing ones, as create / edit-merge / delete operations validated against a schema. Every commit
  writes **`memory_diff.json`** — adds, updates with before/after, deletes with the deleted content,
  and skipped operations with reasons.
- **Read path — changed.** The current retrieval doc states that directory priority queues and
  score propagation were **removed**; retrieval "does not navigate directories recursively". It is
  one vector search per query, scoped by URI subtree, type, ACL and metadata. `find()` is quick
  (no rerank); `search()` first has a language model plan the query into typed sub-queries
  (memory / resource / skill, with intent and priority), then optionally reranks on the L1 text.
  The "visualised retrieval trajectory" of earlier versions is not in the current docs
  *(unverified whether it still exists)*.
- **Storage.** Content in a filesystem abstraction (local, S3, memory); a vector index beside it.
  `rm`/`mv` keep the index in sync by URI prefix.
- **Interfaces.** Python/Go/TypeScript SDKs, HTTP server, `ov` CLI (`ls`, `tree`, `find`, `grep`,
  `add-resource`, …), and an **MCP server over Streamable HTTP at `/mcp` with 20 tools**: `find`,
  `search`, `read`, `list`, `tree`, `remember`, `write`, `edit`, `add_resource`, `add_skill`,
  `list_watches`, `cancel_watch`, `grep`, `glob`, `forget`, `health`, `list_users`, `list_groups`,
  `get_acl`, `set_acl`.
- **Strengths.** One URI space for knowledge, memory and skills; Markdown memories a human can edit;
  an explicit write diff; summary freshness. **Weaknesses.** No temporal validity on memories;
  partial summary refreshes on failure (documented); the hierarchy now lives in the summaries and
  in scoping rather than in retrieval.

**What Liquers takes from it:** the filesystem paradigm (a `Key` is a `viking://` URI), tiers, the
category layout, the write diff as an audit record, and scoped search. **What it does better than
OpenViking:** derived tiers are assets with content-hash versions and dependency tracking, so
"freshness propagation" is the asset layer's invalidation, not a bespoke queue with a 10 % threshold.

### 1.2 Karpathy's "LLM Wiki"

Sources: <https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f> (2026-04-04);
implementation <https://github.com/Astro-Han/karpathy-llm-wiki> (an agent skill).

- **Idea.** Instead of re-deriving knowledge on every question (RAG), the language model
  *incrementally builds and maintains a persistent wiki* — "Obsidian is the IDE; the LLM is the
  programmer; the wiki is the codebase". It is deliberately an idea file, not software.
- **Three layers.** *Raw sources* (immutable; read, never edited), *the wiki* (model-owned Markdown:
  summaries, entity and concept pages, comparisons, an overview), *the schema* (`CLAUDE.md` /
  `AGENTS.md`, co-evolved by user and model, defining structure and workflows).
- **Three operations.** **Ingest** — read a source, write a summary page, update `index.md` and the
  10–15 related entity/concept pages, flag contradictions, append to `log.md`. **Query** — read the
  index first, then the pages; answer with citations; **file good answers back** as pages.
  **Lint** — contradictions, stale claims, orphan pages, missing pages and cross-references, gaps.
- **Files.** `index.md`: a categorised catalogue, one line per page; read first on every query;
  claimed to work at ~100 sources / hundreds of pages without embeddings. `log.md`: append-only,
  headings like `## [2026-04-02] ingest | Title`, so `grep "^## \[" log.md | tail -5` is the recent
  activity.
- **Weaknesses** *(inference)*: index-first reading stops scaling at some hundreds of pages without
  a search tool; correctness depends on the model's discipline; one source touches many pages.

**What Liquers takes from it:** this repository *is* an LLM wiki — `CLAUDE.md` and
`DOCS_STRUCTURE_GUIDE.md` are the schema, `specs/` is the wiki, `index.csv`/`index.md` the index,
`docs_index.py --check` the lint. What the pattern lacks and Liquers supplies: the index as a
derived asset (never stale), lint as a command, and search so that index-first reading is not the
only way in.

### 1.3 mem0 (mem0ai/mem0, OSS "v3")

Sources: <https://github.com/mem0ai/mem0> README, `docs/migration/oss-v2-to-v3.mdx`,
`mem0/memory/main.py`; paper arXiv 2504.19413 *(not re-read)*.

- **Model.** A memory is a short natural-language fact in a vector store, with `created_at`,
  `updated_at`, metadata, optional `expiration_date` and scope ids: `user_id`, `agent_id`, `run_id`
  (plus `actor_id` for multi-party chats). v3 adds an entity collection for entity linking.
- **Write path — changed in April 2026.** The published algorithm used two model calls: extract
  candidate facts, then decide ADD / UPDATE / DELETE / NONE against similar existing memories. v3
  replaced it with **single-pass, ADD-only extraction**: "memories accumulate; nothing is
  overwritten", because reconciliation was slow and destroyed information. `infer=False` stores
  messages verbatim.
- **Read path.** Candidates by semantic search only; BM25 and entity matches **rerank** ("a boost
  signal, not a recall expander"); scores fused into [0, 1]. Expired memories hidden unless asked.
  Temporal ranking (current state over past) is a hosted-platform feature, as is graph memory, which
  was **removed from the open-source SDK**.
- **API.** `add`, `get`, `get_all`, `search`, `update`, `delete`, `delete_all`, `history(id)`
  (SQLite), `reset`; a CLI; a hosted platform. **OpenMemory MCP** (local server with dashboard):
  `add_memories`, `search_memory`, `list_memories`, `delete_all_memories` *(unverified against
  current code)*.
- **Weaknesses.** In OSS, ADD-only keeps superseded facts beside current ones, so contradictions
  accumulate unless something ranks by time; atomic facts lose their context.

**What Liquers takes from it:** scoping by *who* and *which run* as folders; per-memory history;
expiry. **The lesson of its reversal:** in-place consolidation by a model is lossy; append-and-
supersede with history is safer. Liquers' content-hash versions and a supersession field make
supersession cheap to record.

## 2. Other systems

| System | Model | Write | Read | Temporal / provenance | Interface (MCP tools) |
|---|---|---|---|---|---|
| **Letta / MemGPT** (<https://github.com/letta-ai/letta-code>) | Core blocks pinned in context; recall (message history); archival (vectors). Letta Code: **MemFS**, memory as a git repository of Markdown, every edit committed | The agent edits its own blocks (`memory_insert`, `memory_replace`, `memory_rethink`); sleep-time / reflection agents rewrite memory in a git worktree and merge back | Core always in context; `conversation_search`, `archival_memory_search` | Git history | SDK, REST, CLI *(classic details unverified)* |
| **Zep / Graphiti** (<https://github.com/getzep/graphiti>) | Episodes (raw, provenance) → entity nodes and fact edges | Model extracts entities and edges per episode | Hybrid semantic + BM25 + graph traversal, rerankers | **Bi-temporal edges** (`valid_at`/`invalid_at` event time, `created_at`/`expired_at` transaction time); contradicted facts invalidated, not deleted; each edge lists its episodes | `add_memory`, `add_triplet`, `search_nodes`, `search_memory_facts`, `get_episodes`, `delete_episode`, `get_entity_edge`, `delete_entity_edge`, `summarize_saga`, `build_communities`, `clear_graph`, `get_status`, … |
| **Anthropic memory tool** `memory_20250818` (<https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool>) | Files under `/memories`, stored by the client application | The model, explicitly | Browse and read only — no search | None built in; docs recommend expiring unread files | Not MCP: `view`, `create`, `str_replace`, `insert`, `delete`, `rename` |
| **Claude Code memory** (<https://code.claude.com/docs/en/memory>) | `CLAUDE.md` hierarchy (managed, user, project, local) + auto memory: `MEMORY.md` index (first 200 lines / 25 KB loaded) + one topic file per memory, front-matter `type` ∈ user/feedback/project/reference | The model, during work; `modified` timestamp stamped on write | Index always loaded; topic files on demand | `modified` only | File tools |
| **Basic Memory** (<https://github.com/basicmachines-co/basic-memory>) | Markdown files are entities: front-matter (`title`, `type`, `permalink`, `tags`), observations `- [category] text #tag`, relations `- rel [[Target]]` | The model via tools; files are truth, indexed in SQLite/Postgres | Hybrid full-text + vectors; `build_context` over `memory://` links | None | `write_note`, `read_note`, `edit_note`, `move_note`, `delete_note`, `view_note`, `search_notes`, `recent_activity`, `list_directory`, `build_context`, `schema_infer`, `schema_validate`, `schema_diff`, project tools |
| **LangMem** (<https://github.com/langchain-ai/langmem>) | Semantic (collection or profile), episodic, procedural | Hot path (manage/search tools) or background manager that extracts, updates, consolidates | Similarity × importance × strength (recency, frequency) | Invalidation possible | Library tools, not MCP |
| **Cognee** (<https://github.com/topoteretes/cognee>) | Knowledge graph + vectors, custom ontologies | `remember`, `improve` (session cache → graph) | `recall` with routing | — | `remember`, `recall`, `forget` |
| **Supermemory** (<https://github.com/supermemoryai/supermemory>) | Facts distinct from documents; profile static/dynamic | Automatic | Hybrid | New facts supersede old; time-bound facts expire | `memory`, `recall`, `context` |
| **MCP reference memory server** (<https://github.com/modelcontextprotocol/servers/tree/main/src/memory>) | Entities with observations, typed relations; one JSONL file | The model, by hand | `search_nodes`, `open_nodes`, `read_graph` | None | 9 graph-editing tools |
| **A-MEM** (arXiv 2502.12110) *(unverified)* | Zettelkasten notes with keywords, tags, links | Model links new notes and updates neighbours | Embedding neighbours + links | — | Research code |

## 3. Patterns and tensions

1. **Files versus databases.** File/Markdown systems (OpenViking, LLM wiki, Claude Code, Basic
   Memory, Letta MemFS, the memory tool) win on editability, review, versioning and audit.
   Database systems (mem0, Zep, Supermemory) win on temporal reasoning and benchmark scores. No
   system combines Graphiti-grade validity and provenance with file-native editing.
2. **Who extracts.** Either the *service* runs a model (mem0, OpenViking, Zep, Cognee, Supermemory)
   or the *calling agent* writes deliberately (memory tool, Claude Code, Basic Memory, LLM wiki,
   MCP reference server, Letta's hot path). The first needs model credentials and is
   non-deterministic; the second costs the agent's context but keeps the service simple and
   testable.
3. **Mutate versus supersede.** mem0 abandoned in-place model-driven updates as lossy; Zep
   invalidates instead of deleting; OpenViking mutates but records a diff. The safe pattern is
   append-or-edit **with history and an explicit supersession link**.
4. **Indexes go stale.** Every system with an index or summary has a freshness mechanism or a
   failure mode for its absence: OpenViking's propagation threshold, the LLM wiki's lint, Claude
   Code's size budget.
5. **Tool surfaces converge.** Every MCP surface has *search*, *read*, *list/browse*, *write*,
   *edit*, *delete/forget*; the larger ones add graph or schema tools and administration. Basic
   Memory's `schema_validate` and the LLM wiki's *lint* are the only consistency tools.
6. **Retrieval tracing is rare.** OpenViking's trajectory view (status unclear), mem0's `explain`,
   OpenViking's `query_plan` in results. Most systems return hits with no reason.

## 4. Services a successful memory system provides

Who provides each (OV OpenViking, KW LLM wiki, M0 mem0, LT Letta, ZG Zep/Graphiti, AMT memory tool,
CC Claude Code, BM Basic Memory, LM LangMem, CG Cognee, SM Supermemory, MR MCP reference server).
The **Liquers** column says what already exists at `main` (search merged 2026-10-12).

| # | Service | Provided by | Liquers today |
|---|---|---|---|
| S1 | **Capture / ingest** of raw material, kept immutable beside what is derived | OV, KW, M0, ZG, CG, SM | Store writes; resources; no ingest commands |
| S2 | **Extraction** of durable memories (by the service's model, or by the agent) | OV, M0, ZG, LM, CG, SM *(service)*; AMT, CC, BM, KW, MR *(agent)* | — |
| S3 | **Organisation**: categories, scopes, schemas per kind | OV, CC, LM, BM, M0 (scopes) | Keys/folders; `specs/` schema in prose + `docs_index.py` |
| S4 | **Tiered representation** (abstract → overview → full) | OV, KW, CC, LT, ZG | `title`/`description` in metadata; front-matter in `specs/` |
| S5 | **Retrieval** — lexical, metadata filter, scope | all; lexical: ZG, BM, OV, M0 (boost) | `ns-search` (BM25, fields, regex, sections) on the search branch |
| S6 | **Retrieval** — semantic | OV, M0, ZG, BM, LM, CG, SM | — (`external-index-sync`) |
| S7 | **Retrieval** — links / graph | ZG, CG, BM, MR | — (`NO-MARKDOWN-NAVIGATION-COMMANDS`) |
| S8 | **Consolidation**: dedup, merge, update | OV, LM, LT, KW, SM; M0 removed it | `docs_index.py new` duplicate search |
| S9 | **Temporal validity / supersession** | ZG (best), SM, M0 (expiry), KW (lint) | `status`, `superseded_by`, archive genre, `reviewed:` age; asset `expires` |
| S10 | **Provenance** — what a memory came from | ZG, OV, KW, M0 | Asset `dependencies`, `query`; `Discovery` section by convention |
| S11 | **History / versioning / write audit** | OV (diff), M0 (`history`), LT and KW (git), ZG | Content-hash `version`; git |
| S12 | **Forgetting / expiry / decay** | M0, SM, LM, OV (`forget`) | Asset expiration (`expires`) |
| S13 | **Lint / consistency** | KW, BM, CC, OV (op validation) | `docs_index.py --check`, `validate_phase.py` |
| S14 | **Index / catalogue / log** | KW (`index.md`, `log.md`), CC (`MEMORY.md`) | `index.csv`, `index.md` (generated) |
| S15 | **Derived-content freshness** | OV (propagation) | **The asset layer** — versions, dependencies, recipes |
| S16 | **Human editability** | OV, KW, CC, AMT, BM, LT | Plain files in git |
| S17 | **Procedural memory / skills** | OV, LT, LM, CC (skills) | `.claude/skills/` |
| S18 | **Context budgeting** — limits, truncation signals | OV, CC, LT, SM | `ns-search` `limit`, "N of M" description |
| S19 | **Observability** — why a hit, what a write changed | OV, M0 (`explain`), KW (`log.md`) | `matched` + `excerpt` columns; no write log |
| S20 | **Scoping and identity, ACL** | OV, M0, ZG, SM | Out of scope (`CORE-SESSION-AND-KEY-ACL`) |
| S21 | **Access**: MCP, CLI, HTTP, SDK | all; MCP in OV, ZG, BM, M0, CG, SM, MR | `liquers-axum`; Python bindings; no MCP |
| S22 | **Background processing** so memory work adds no latency | OV, LT, LM, CG, ZG | Asset job queue |

Liquers is unusually strong on S15 (no other system has general derived-data invalidation), S5
(merged 2026-10-12), S11/S16 (versions plus git) and S13/S14 (this repository's own
practice). It is missing S2, S6, S7, S19's write log, and an MCP surface (S21) — which, with a
specification of S3, S8 and S9 as conventions, is what this design is for.
