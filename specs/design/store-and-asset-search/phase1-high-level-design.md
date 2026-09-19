# Phase 1: High-Level Design — Store and asset search

## Feature Name

Store and asset search

## Purpose

Let a caller ask a Liquers environment **which records match** instead of listing a subtree and
filtering in their own code. Stores and the asset layer can enumerate and fetch but cannot select
(`STORE-NO-CONTENT-OR-METADATA-SEARCH`), so every consumer pays O(corpus) reads and reimplements
retrieval. Two consumers are essential: **the agent memory system**, which needs to find and judge
documents without spending context on them, and **a user's search field**, which needs one input box
to return a usable list.

> **Amended 2026-09-19 by the split.** The record half of this design was extracted into
> [`record-streams`](../record-streams/) and is being stabilized first. Everything below about
> *records, streams, chunks, batches and schemas* is now that design's, and is restated here only as
> the context a search needs. This design owns the **predicate** and its syntax. The sentence
> "selection is a trait method" below was also reversed by Phase 2 revision 2 — selection is a
> command over a record stream, and no store or asset trait gains a method.

## The model

Every essential use case is the same operation: *select records from a set, by a predicate over
their fields and their text, and return enough of each record to judge it and to address it.* They
differ only in **where records come from** and **which clause is used**. Fixing those two contracts
is the whole design:

- **Record sources** are an open set — a store subtree, an asset listing, the command registry, and
  later a table's rows or a document's chunks.
- **Predicate clauses** are an open set — text and fields now; vector similarity later, as
  additional clauses over the *same* records.
- **Projection is a command, selection is a trait method.** Projection varies with the value type,
  which is open by design; selection varies with the backend.
- **External systems are materialized views, reconciled rather than notified.** A search engine, a
  vector store, a RAG pipeline and an external SQL database differ only in what they answer; the
  feed and the freshness are identical, so one layer serves all four.
- **A chunk is the unit of refresh, a batch the unit of memory, a record the unit of retrieval.**
  A stream query yields a partition of chunks — split by *dependency*, so one changed file
  re-derives one chunk — and a chunk is delivered as batches, split by *size*, because the unit that
  is natural for dependencies may be far too large to hold. One partitioning mechanism, two criteria.
- **Records are a tabular interchange layer, not only search infrastructure.** A chunk is a table
  and a stream is a table in parts, so the same mechanism serializes to NDJSON, CSV or parquet and
  presents a corpus to SQL.

## Scope

**Essential — the first version is not done without these:** free-text and field predicates together
over one record set, with a small search syntax for the user's box; store entries, live assets and
recipe-declared keys unioned; command discovery as a further record source; results carrying an
address, the fields to judge by, why they matched and a reserved score; a bounded search with
truthful truncation; a command surface so the UI field, HTTP and MCP are the same mechanism; **a
minimal built-in capability depending on nothing external — at least metadata**; **an
interoperability layer** that can plug in an external search engine, vector store or RAG pipeline
without the design tying itself to any one of them; and a baseline that runs on wasm.

**Optional — reachable later, not built now:** ranked full text; semantic search and RAG; a
maintained index, whether per-document filters or a monolithic one; any actual external sink; tags;
facets and ranges; router fan-out across mounts; a dedicated HTTP endpoint.

**Milestones:** M0 foundations, M1 metadata search, M2 text search and the search box, M3 asset union
and command discovery — one coherent deliverable, and **exactly what the agent memory MVP needs**. M4
(a per-document filter index) is an optional performance step that changes no contract. M5–M7 —
record streams, batching and streaming, the interoperability layer with a first sink — are separate
efforts the foundations admit. [`roadmap.md`](./roadmap.md) §2–§3.

**Prerequisites, none blocking.** `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (P2) affects the MVP's
quality, but narrowly: measured on this repository's 321 tracked documents, a key prefix does the
biggest single cut for free and only *lifecycle state* — open versus closed — genuinely needs a
metadata field (`roadmap.md` §3). Building M1 so field predicates resolve **by name against the
record** makes fixing it a pure upgrade. `CORE-STORE-OPENBIN-MISSING` (P3) and
`VALUE-SERIALIZATION-HAS-NO-INCREMENTAL-WRITER` (P2) bound how far streaming can go at M6;
`ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (P2) costs M7 latency, not correctness.

**Separate task — considered only where it intersects:** SQL. An external SQL database is fed and
kept fresh by the same interoperability layer as a search engine or vector store, so the intersection
is the feed. The one requirement it imposes here is that records carry **named, typed fields**, so a
SQL column and a predicate field are the same thing. Its query language, joins, schema mapping and
write-back are not designed here.

**Not search:** link traversal, dependency-graph queries, and DataFrame filtering. Each has, or
deserves, its own mechanism; folding them in grows the predicate without improving an essential case.

**Hard invariants:** a search never starts an asset — it obtains and describes assets without
triggering their recipes, and reports `Status::Recipe` as an honest answer rather than resolving it.
**Indexation may evaluate; search may not** — a search is a user's question with a latency budget,
while indexation is a scheduled job whose scope and cost its owner accepted. A search is bounded and
reports truncation; its result is an addressable value; results are unordered unless a scoring clause
was used; and the baseline runs everywhere, wasm included. Finally, **an external system's
correctness comes from reconciliation, never from a delivered notification** — push is a latency
optimization with no correctness role.

## Core Interactions

**Query system** — no grammar change. A search is an ordinary action whose argument carries the
predicate or a short search syntax; `ActionRequest::encode` escapes arbitrary terms.

**Store system** — this proposed a selection method on `AsyncStore` with a default scan, a
capability flag and conformance rules. **Phase 2 revision 2 drops all of it from the MVP.** Once
records are produced by *commands*, a trait method is a push-down optimization rather than the
mechanism — valuable when a backend can filter without materializing, absent everywhere today, and
addable later without changing a consumer. The consequence is stated there: this work does not close
`STORE-NO-CONTENT-OR-METADATA-SEARCH`, which asks for selection on the store. Router fan-out across
mounts is likewise deferred, since there is no store-level search to fan out.

**Interoperability** — a partition of `(chunk id, version, refresh query)` plus a per-chunk fetch, a
reconciliation diff, and a staleness declaration. It needs almost no new vocabulary: `Version` is
already a content hash, `DependencyRecord { key, version }` is already the shape a chunk version
hashes over, `Expires` already expresses how stale a view may be, and `ExpirationMonitor` is already
a background worker of this shape. One new concept is required — an identity for the *projection
rule*, carried in the chunk version, so that changing a tokenizer or an embedding model invalidates
an external index instead of leaving it looking fresh.

The **push path does not exist yet**: expiration is notified, but only on a channel you can reach by
already holding the asset, only for live assets, and through a `watch` that retains the latest state
rather than the sequence. Filed as `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET`
(P2). It blocks nothing — reconciliation is the guarantee, and this layer is the roadmap's last
milestone — but until it is fixed, every external view is as stale as its polling interval.

**Command system** — a namespace in `liquers-lib` building the predicate from arguments; further
filtering is ordinary commands over the returned list. Separately, the **command registry becomes a
record source**, so "which commands mention resize" needs no command-specific search code.

**Indexation policy** — two decisions that only arise once something is written down ahead of a
query. *Which* documents are indexed: the scope or configuration query, plus key patterns, plus type
rules, with a per-asset opt-out deferred until attributes exist — and exclusion is explicitly not a
security mechanism. *What content* is indexed: metadata-only, when-ready (the safe default), or
produced at indexation time. **Metadata is always indexable**, so a document is discoverable without
anything being computed. Four classes decide the content policy, differing on whether a
document's version is knowable without producing it: **stored** (yes, content hash); **transient** —
deterministic, cheap, deliberately not stored (yes, derived from its dependencies, so staleness is
decidable without producing anything); **ad-hoc query** — a non-keyed asset such as a report
generated on the fly (yes, derived offline from the plan's dependencies and command versions, but
with no `listdir` to enumerate it, so the set must be *declared* — which the partition already does);
and **volatile** (no version at all — `assets.rs:5583` gates registration on
`Ready | Source | Override` — so it needs time-based refresh via `Expires` and can only ever be
snapshotted). The transient class is the clean case for
*produce*, and Liquers cannot express it today: `CommandMetadata.cache` is read by nothing,
`register_command!` cannot set it, and `PersistenceStatus::NotPersisted` means the write failed
(`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`). The MVP needs no configuration: a scan that
may not evaluate arrives at *when-ready* for free. See [`indexation-policy.md`](./indexation-policy.md).

**Asset system** — search unions store entries, live assets and recipe-declared keys the way
`AssetManager::get_asset_info` resolves them, but without triggering evaluation. That needs a fix:
today a *live* key is routed through `AssetManager::get`, which submits to the job queue when the
asset is unfinished and cannot fast-track, so describing an asset in `Status::Recipe` runs it
(`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`, P1). The non-triggering pieces already exist —
`lookup_key_asset`, and `AssetRef`'s `status`, `poll_state`, `get_any_status` and `get_asset_info` —
they are simply not recognised as the safe path.

**Value types** — a record type: identity as **(asset, record id)**, named typed fields carried as
the existing `Value::Object`, and text. The representation is not new; what the type adds is a
guaranteed address and a field/text distinction a bare map cannot express. The identity is a pair
rather than a stored query because holding a query string per row costs a multiple of the data it
describes: the asset is carried by the chunk, the id is small and asset-dependent (a row number, a
line, a pointer, or nothing when the asset *is* the record), and the evaluable locator is derived on
demand through `ActionRequest` — never by string templating.
`Value::AssetInfo(Vec<AssetInfo>)` and `Value::CommandMetadata` remain the shapes a search result and
a command description take.

**Scope of the record model in this design** — the first version needs only **Level 0**: one record
per asset, no record id, no chunks, no batches, no schema. That is what agent memory and a user's
search box are about — finding documents, not rows. **Level 1** — many records per asset, with ids,
chunks, batches, locator rules and schema — is what SQL, external sinks, serialization and searching
*inside* structured data need, and all of those are optional.

**The level is cardinality only.** Where a record's *fields* come from is a separate axis: metadata,
a materialized projection, or a projection applied inline. Extracting front-matter yields one record
per asset, so it is Level 0 despite not reading metadata. Inline projection is permitted — applying a
command to a state creates no asset, and a text clause already reads the same bytes, so the parse is
cheaper than the match performed on them. Materializing a projection into metadata is therefore an
**optimization** that removes the content read, not a precondition. The dangerous operation is not
the projection but *starting an asset*, which is why the invariant is stated about the asset manager
above.

Level 1 arriving as specialized commands would put the whole contract's specification on Level 0,
which is a fair worry and a smaller one than it looks: [`roadmap.md`](./roadmap.md) §1 shows that
**exactly seven decisions are foundational** — the record's shape with the id field present but
unused, identity as a pair, fields as a named `Value::Object`, a bounded opaque result rather than a
`Vec`, the ordering promise, the text/field distinction, and non-exhaustive enums. Schema, chunks,
batches, locator rules, streaming, projection identity and the reconciliation contract are all
additive, because those seven left room for them. The rest of `record-model.md` specifies where the
design is going, which is what keeps Level 1 from being a redesign — not what the first version must
contain.

**Built-in engine** — a scan in the first version: field tests against metadata, text tests against
bytes only when a text clause demands them. The designed second step is a per-document word filter —
tinysearch's data structure, not tinysearch itself, because it builds its index at build time and
emits a compiled wasm module. A per-document filter is a derived asset of *one* document, so it has
no dependency fan-out and search becomes filter-then-verify, with the verify stage producing the
snippet that was owed anyway.

**Web/API and UI** — nothing new. The command is reachable through `/q`; the user's search field is a
UI input whose value is substituted into a search query, which is already how UI elements work; an
MCP tool builds a query rather than growing its own engine.

## Crate Placement

`liquers-core` — the record and predicate types, the store selection method with its default
implementation, the asset-level union, capability and conformance rules. `liquers-lib` — the command
namespace and the search-syntax front end. No change to `liquers-store` beyond an optional push-down
override; none to `liquers-axum`.

## Documentation Intent

**Reference:** create `specs/reference/SEARCH.md` — the record, the predicate, the match and
ordering contracts, the invariants, and what a store guarantees when it overrides the default. New
rather than an extension: no existing reference owns selection, and splitting it between
`STORE_SEMANTICS.md` and `ASSETS.md` leaves the union unowned.

**Guide:** extend `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` with the selection capability, where a
store author already learns which capabilities to declare and how to run the conformance suite. A
second guide — how to connect an external engine — becomes justified the moment a real sink exists;
until then the reference carries the contract. Reconsider a user-facing search guide if the search
syntax needs teaching beyond a paragraph.

**Other documents to create:** none.

**Specific documents to update:** `specs/reference/STORE_SEMANTICS.md` (selection contract, new
capability), `specs/reference/CONFORMANCE_TERMS.md` (new rule terms), `specs/README.md` (capability
map), `specs/command_registry.yaml` (regenerated, not edited).

Audience: a contributor implementing or overriding selection, and an agent or user composing a
search. Both should work from the reference and the guide without opening this folder.

## Open Questions

1. Does the union prefer the live asset over the store for a key present in both, and is that the
   precedence `get_asset_info` already uses?
2. Is a partially pushed-down predicate re-checked by the caller, or must push-down be
   all-or-nothing? A dropped clause is a correctness bug either way.
3. How is a record's text obtained — from bytes for text media types, or through a projection
   command — without reading every byte in the corpus to find out?
4. Is a hit's identity always a query, and how does a store key render as one?
5. Is truncation a hard limit with a flag, or a cursor — and if a cursor, stable against what?
6. Command discovery: a record source over the registry, or a read-only virtual store that makes
   commands enumerable by `listdir` with nothing command-specific in the search path?
7. Is router fan-out in the first version, or does a root above a mount boundary get refused until
   merge semantics are settled?
8. How does a projection rule get an identity that participates in the reconciliation diff, and does
   it belong on the record, the sink, or both?
9. Does the reconciliation contract ship alongside the scan or after it? It should ship with a test
   double either way — a trait with one implementation is a guess.
10. What exactly does the small search syntax accept, and what does it do with input it does not
   support: treat it literally, or report a parse error?
11. Is a chunk version derived generically from its query's dependency set, or computed by the
   stream command? Generic derivation is much better if possible — it cannot be got wrong per
   command.
12. Is the partition itself a record stream (one mechanism, recursive base case) or a separate,
   lighter type?
13. Is `text` a field carrying the `text` role, or a distinct part of the record?
14. Are batches addressable as queries, enumerated by count, or cursor-driven? In process a chunk can
   be a genuine iterator; across a query boundary a query returns a materialized value, so the
   streaming form has to be addressable batches.
15. With four consumers — search, external sinks, SQL, serialization — does the record model graduate
   to its own design once Level 1 is needed, with search as its first client?
16. Is the non-triggering describe the default for `get_asset_info` with an opt-in resolving
   variant, or do the two get separate names
   (`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`)?
17. How is a projection's purity guaranteed on the search path — a declaration on the command, or a
   restricted context that refuses evaluation (`COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`)?
18. Does `produce` exist in the first indexing version? Leaving it out is smaller, but the transient
   class makes a whole category of cheap derived views permanently unsearchable by content, whose
   only workaround is storing documents whose point is not being stored.
19. What does an index entry record for a volatile document, and is it marked so a consumer knows its
   freshness is time-based rather than version-vouched?
20. How is a declared set of ad-hoc queries bounded, and are two spellings of the same computation
   deduplicated?
21. How is "why is this document not in my results?" answered for one key? Silent absence is very
   hard to debug.
22. ~~How does a bare field name resolve when two layers define it?~~ **Resolved in Phase 2:** field
   names are qualified at projection time (`meta.`, `attr.`, `key.`); the matcher compares qualified
   names exactly; an ambiguous unqualified name is a parse-time error naming both candidates. The
   original question, for the record: `status` is the **asset**
   lifecycle in `MetadataRecord` and the **document's** lifecycle in `specs/` front-matter, and both
   would answer `status:draft` silently. Namespaced fields, or a documented precedence — cheap now,
   confusing forever if left.

## References

- [Roadmap](./roadmap.md) — the seven foundational decisions, the milestones, and the agent-memory
  MVP cut
- [Use cases](./use-cases.md) — the survey and the essential/optional classification
- [Research questions](./research-questions.md) — the nine questions, answered with evidence
- [Options analysis](./options-analysis.md) — ground truth, the model, the axes, the recommendation
- [Record model](../record-streams/record-model.md) — what a record, a stream, a chunk and a schema are, and why
  the refresh unit is the chunk
- [Indexation policy](./indexation-policy.md) — which documents are indexed, whether content is
  produced or only read, and why volatile assets need time-based refresh
- [Interoperability layer](./interoperability-layer.md) — feeding and reconciling external search
  engines, vector stores, RAG pipelines and SQL mirrors with one mechanism
- `specs/issues/STORE-NO-CONTENT-OR-METADATA-SEARCH.md` — the gap this closes
- `specs/design/agent-memory-mvp/phase1-high-level-design.md` — its open question 3 asks how far an
  MVP goes on search; this design answers it
- `specs/issues/DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED.md` — filed while
  writing the analysis; it is what makes a maintained index an unverified option rather than a ready
  one
- `specs/issues/CORE-METADATA-NO-APPLICATION-ATTRIBUTES.md` — adjacent gap; field lookup is by name
  against a record, so tags become a field the day it is fixed
- `specs/reference/STORE_SEMANTICS.md`, `specs/guides/STORE_IMPLEMENTATION_GUIDE.md` — the contract
  and the author-facing procedure this extends
