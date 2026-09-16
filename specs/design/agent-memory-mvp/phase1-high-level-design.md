# Phase 1: High-Level Design — Agent Memory Service (MVP)

## Feature Name

Agent memory service (MVP)

## Purpose

Serve a document corpus — starting with this repository's own `specs/` and `.claude/skills/` — as
tiered, searchable memory for coding agents, through the assets API and an MCP adapter. Liquers
already supplies what such a system is normally built to provide: a query is an address *plus* a
derivation, and the asset layer recomputes a derived entry when its source's content hash changes,
so summaries and indexes cannot go quietly stale.

## Core Interactions

### Query System

No grammar change. The corpus is addressed as `-R/mem/…`; derivations are ordinary action chains.

### Store System

`AsyncStoreRouter` mounts the corpus and an agent-writable area under one `mem/` prefix. Stores are
the storage layer only — they are not the interface a client talks to.

### Command System

A new `ns-mem` namespace, for populating asset metadata from document front-matter, and for
selection and search. Writes go through a command that reaches the store or asset manager via
`Context::get_async_store` / `Context::get_asset_manager`, not through a raw store `POST`.

### Asset System

The client interface. `AssetInfo` already carries `title` and `description`, which **are** the L0
and L1 tiers — no tier-accessor commands are needed, and one `listdir` describes a whole directory
without reading any data. Blocked on `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0): six of the
ten assets endpoints are 501 stubs, `listdir` among them. That gap is fixed first.

### Value Types

None new. The corpus is text and bytes.

### Web/API

No new endpoints — the existing assets API, once implemented, is the surface. An MCP adapter maps a
small fixed tool set onto it; the query language, not a 300-entry tool list, carries the
composability.

### UI

Not applicable.

## Crate Placement

`liquers-axum` — complete the assets API. `liquers-lib` — the `ns-mem` namespace. A new
`liquers-mcp` crate — the MCP adapter, which is not HTTP and does not belong in `liquers-axum`.
No `liquers-core` change is expected beyond whatever the assets API needs on `AssetManager`.

## Documentation Intent

**Reference:** create `specs/reference/AGENT_MEMORY_SERVICE.md` — the key layout, the tier
contract (`title` = L0, `description` = L1, data = L2) and the MCP tool surface. A new document
rather than an extension: no existing reference owns "what a memory corpus is".

**Guide:** create `.claude/skills/liquers-memory/` — the audience is a coding agent using the
service, which is what skills are for, not a `guides/` document aimed at contributors.

**Other documents to create:** none.

**Specific documents to update:** `reference/WEB_API_SPECIFICATION.md` §5.1 if implementing the
assets endpoints changes their specified semantics; `specs/README.md` capability map; `CLAUDE.md`
if the service becomes the normal way agents read `specs/`.

Audience: a coding agent reading the corpus, and a contributor running the service. Both should
work from the reference and the skill without opening this folder.

## Open Questions

1. `AssetManager::get_asset_info` already resolves a key as live asset, else store, else recipe
   provider — so an asset listing is the union, and a never-evaluated recipe key is described from
   its recipe. Is that the contract the memory service wants, and is it what
   `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` should specify?
2. Where do `title` and `description` come from for a file store over `specs/`? A recipe that
   derives metadata from front-matter, a metadata-populating command run on write, or a store that
   reads front-matter itself.
3. How far does the MVP go on search, given `STORE-NO-CONTENT-OR-METADATA-SEARCH`? A command over
   a subtree is honest at ~300 documents and wrong at 100×.
4. Does the agent-writable area live in the same store as the corpus, and what stops a note from
   contradicting `specs/`?
5. Is the acceptance test still "regenerate `specs/index.csv` from a query and diff against
   `scripts/docs_index.py`"? It is a real oracle, but it may pull the MVP toward index generation
   and away from retrieval.

## References

- `specs/issues/AGENT-MEMORY-SERVICE.md` — the feature this design serves
- `specs/issues/AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED.md` — the P0 blocker
- `specs/issues/STORE-NO-CONTENT-OR-METADATA-SEARCH.md`,
  `specs/issues/CORE-METADATA-NO-APPLICATION-ATTRIBUTES.md`,
  `specs/issues/STORE-WRITE-HAS-NO-PRECONDITION.md` — gaps the MVP works around
- `specs/reference/WEB_API_SPECIFICATION.md` §5 — the assets API as specified
- [OpenViking](https://github.com/volcengine/OpenViking) — the reference model for tiered,
  filesystem-shaped agent context
