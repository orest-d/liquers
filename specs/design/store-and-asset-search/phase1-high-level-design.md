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

**Separate task — considered only where it intersects:** SQL. An external SQL database is fed and
kept fresh by the same interoperability layer as a search engine or vector store, so the intersection
is the feed. The one requirement it imposes here is that records carry **named, typed fields**, so a
SQL column and a predicate field are the same thing. Its query language, joins, schema mapping and
write-back are not designed here.

**Not search:** link traversal, dependency-graph queries, and DataFrame filtering. Each has, or
deserves, its own mechanism; folding them in grows the predicate without improving an essential case.

**Hard invariants:** a search never evaluates; it is bounded and reports truncation; its result is an
addressable value; results are unordered unless a scoring clause was used; the baseline runs
everywhere, wasm included; and **an external system's correctness comes from reconciliation, never
from a delivered notification** — push is a latency optimization with no correctness role.

## Core Interactions

**Query system** — no grammar change. A search is an ordinary action whose argument carries the
predicate or a short search syntax; `ActionRequest::encode` escapes arbitrary terms.

**Store system** — a selection method on `AsyncStore` with a default scan over `listdir_keys_deep`,
so no existing store breaks and a capable backend can override. Needs a `StoreCapabilities` flag and
conformance rules. Router fan-out across mounts below a root is specified now, implemented later.

**Interoperability** — a feed of `(record id, version)` plus a fetch, a reconciliation diff, and a
staleness declaration. It needs almost no new vocabulary: `Version` is already a content hash on
every stored record, `DependencyRecord { key, version }` is already the shape of "what I observed",
`Expires` already expresses how stale a view may be, and `ExpirationMonitor` is already a background
worker of this shape. One new concept is required — an identity for the *projection rule*, so that
changing a tokenizer or an embedding model invalidates an external index instead of leaving it
looking fresh.

**Command system** — a namespace in `liquers-lib` building the predicate from arguments; further
filtering is ordinary commands over the returned list. Separately, the **command registry becomes a
record source**, so "which commands mention resize" needs no command-specific search code.

**Asset system** — search unions store entries, live assets and recipe-declared keys as
`AssetManager::get_asset_info` already resolves them, without triggering evaluation.

**Value types** — none new. `Value::AssetInfo(Vec<AssetInfo>)` and `Value::CommandMetadata` already
exist; a per-hit match record sits beside the asset info rather than inside it.

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

## References

- [Use cases](./use-cases.md) — the survey and the essential/optional classification
- [Research questions](./research-questions.md) — the nine questions, answered with evidence
- [Options analysis](./options-analysis.md) — ground truth, the model, the axes, the recommendation
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
