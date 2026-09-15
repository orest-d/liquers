---
id: AGENT-MEMORY-SERVICE
kind: feature
title: A memory service for agents, built on the liquers stack
status: draft
priority: P2
complexity: XL
area: [axum, lib/commands, docs]
design: agent-memory-mvp
created: 2026-09-15
github:
---
## Problem

Coding agents working on this repository have no service to read from. What they have is a
filesystem and a set of conventions written in `CLAUDE.md`: read `specs/index.csv`, grep
`specs/issues/`, open the design folder named in a front-matter field, do not edit
`specs/archive/`. Every one of those conventions is enforced by prose and by
`scripts/docs_index.py --check`, which is a single-purpose, hand-rolled indexer for one corpus on
one machine.

That arrangement has three costs.

1. **Retrieval is all-or-nothing.** An agent that wants to know whether an issue is relevant must
   read the whole issue. Nothing serves the cheap summary that `MetadataRecord.title` and
   `.description` are already shaped to hold, so context is spent on documents that turn out not
   to matter.
2. **It is local-only.** Nothing outside a checkout can ask what the project knows, and nothing
   persists across a session except what was committed.
3. **The mechanism is not reusable.** `docs_index.py` implements indexing, front-matter parsing,
   vocabulary validation and link checking in Python, beside a Rust project whose entire purpose
   is addressable, derived, cached data.

Liquers already has the substrate: a store is the filesystem, a `Key` is the path, a `Query` is a
path *plus a derivation*, a recipe declares a derived key, and the asset layer caches it with
content-hash versions and dependency tracking so it recomputes when its source changes. What is
missing is the corpus-facing command namespace, a service binary, a tool surface an agent can
call, and the written contract for all three.

## Impact

Affects agents and contributors working on this repository first, and any Liquers deployment that
wants to serve a document corpus second. There is a workaround — the current conventions — and it
works, which is why this is P2 rather than higher. What it does not do is scale past one
checkout, and it spends agent context on full documents where an abstract would do.

It is also the clearest dogfooding target the project has. `specs/` is a corpus of ~300 tracked
documents with a real schema, a real consumer and an existing generator to measure against.

## Expected behaviour

A memory service composed of existing Liquers parts:

- Stores hold the corpus and an agent-writable area, mounted under a `mem/` prefix by
  `AsyncStoreRouter`. Storage only — not the interface a client talks to.
- **The assets API is the client interface.** It answers about assets rather than about stored
  bytes, so it can report evaluation status, volatility and expiration, and can address a computed
  query. Six of its ten endpoints are 501 stubs today, `listdir` included, which makes
  `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0) this feature's first prerequisite.
- **The tiers already exist in metadata.** `MetadataRecord` and `AssetInfo` both carry `title` and
  `description` — L0 and L1 respectively, with the data as L2 — so one `listdir` describes a whole
  directory without reading any of it. What is needed is a way to *populate* those fields for
  documents that arrive without them, not a new set of tier-accessor commands.
- A `ns-mem` command namespace covers that population, plus selection and search. Writing is a
  command reaching the store or asset manager through `Context`, not a raw store `POST`.
- Recipes declare derived entries, and the asset layer invalidates them when a source's
  content-hash version changes.
- An MCP adapter and a skill make the service reachable by an agent.

The design is in [`design/agent-memory-mvp/`](../design/agent-memory-mvp/), following the
`liquers-project` five-phase workflow. Phase 1 is written and awaiting approval; it names the
blocker above and the three gaps the MVP is meant to work around without fixing
(`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`, `STORE-NO-CONTENT-OR-METADATA-SEARCH`,
`STORE-WRITE-HAS-NO-PRECONDITION`).

## Discovery

Raised 2026-09-15 as an analysis question — what would it take to turn Liquers into an agent
memory system comparable to OpenViking, using `liquers-axum` as the interface, stores for storage
and commands as tools. The analysis is the design folder; this record is the backlog item it
leaves behind.
