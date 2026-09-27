---
id: STORE-AND-ASSET-SEARCH
kind: design
title: Search over stores and assets
workflow: liquers-project
status: draft
phase: architecture
area: [core/store, core/assets, core/commands, lib/commands, docs]
gh_pr: []
issues: [STORE-NO-CONTENT-OR-METADATA-SEARCH]
affects_docs: []
created: 2026-09-17
superseded_by:
---
# Search over stores and assets — design tracking

**Created:** 2026-09-17

## Phase Status

- [x] Phase 1: High-Level Design — approved 2026-09-18
- [ ] Phase 2: Solution & Architecture — written to revision 7, **blocked** on `record-streams`
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Split: records moved out, 2026-09-19

Six Phase 2 revisions established that the interesting half of this work was **not the search**. A
search is a predicate over a stream of records, and the record stream turned out to serve four
consumers of which search is one — search, external engines, SQL and serialization — while carrying
requirements search never raises: multi-gigabyte lazy processing, an Arrow-compatible layout, a
DataFrame role where polars cannot be bundled, and per-chunk provenance.

So the record mechanism was extracted into [`record-streams`](../record-streams/) and is being
**stabilized first**; search is rebuilt on top of it. This design keeps the predicate, its syntax,
its parser, the two execution paths, the indexation policy, the interoperability layer and the
`get_asset_info` repair.

**Before this design advances,** its Phase 2 needs the tidy that `record-streams` Phase 2 received
on 2026-09-20: seven rounds of amendment by changes in the other design have left revision-numbered
commentary through the prose, and the history belongs in one changelog rather than scattered. An
audit on 2026-09-20 found the document otherwise sound — every type it references is defined, and one
stale module path (`liquers-core/src/search.rs`, left behind when the predicate followed records into
`liquers-lib`) was corrected.

**Consequence for sequencing:** `NO-RECORD-STREAM-ABSTRACTION` is now a declared **blocker** on this
design's Phase 2, resolved by the other design completing rather than by a fix here. Phase 2 cannot
be approved while `record-streams` Phase 1 is unapproved — which is the intended order, not an
obstacle.

## Notes

Started 2026-09-17 from a loose brief: make the store and assets searchable, over metadata and
possibly over data, for both people and agents. The brief asked for the task to be **delimited**
first and the **options analysed** second; a second round widened it to a broad use-case survey and
nine research questions, seeking the design sweet spot — the simplest common denominator covering as
many use cases as possible. The folder therefore carries three documents beside Phase 1:

- `use-cases.md` — the survey, with every use case classified **essential**, **optional** or **not
  search**. The two essentials named by the brief are excellent agent-memory support and a simple
  user search field.
- `research-questions.md` — the nine questions answered with evidence: store/asset methods versus
  commands, what search must do, third-party engines and why the Whoosh prototype feels wrong,
  query languages and GlueSQL, command discovery, agent and MCP needs, search syntax standards,
  embeddings in metadata, and semantic search.
- `options-analysis.md` — ground truth at HEAD, the unifying model, nine decision axes, the
  recommended combination, the invariants, and the questions Phase 2 must answer. It names no types
  or signatures; that is Phase 2's job.
- `interoperability-layer.md` — the layer for plugging in an external search engine, vector store,
  RAG pipeline or SQL mirror without tying the design to any of them. Added in the third round,
  when the brief asked whether one hook system could serve them all and handle updates and
  expiration.
- [`record-model.md`](../record-streams/record-model.md) — what a record, a record stream, a chunk, a
  batch and a schema are. Added in the fourth round, when partial refresh turned out to need a unit
  smaller than the stream, and extended in the fifth when memory turned out to need a second, smaller
  one. **Moved to [`record-streams`](../record-streams/) in the split of 2026-09-19** and owned
  there.
- `indexation-policy.md` — which documents are indexed, whether content is read or produced, and why
  volatile assets need a second refresh regime. Added in the seventh round.
- `roadmap.md` — which decisions are foundational and which are additive, the milestones, and what
  the agent memory MVP actually needs. Added in the sixth round against a fair objection: making
  Level 1 "specialized commands" appears to put the whole specification's burden on Level 0.

### The sweet spot

Every essential use case is one operation: *select records from a set, by a predicate over their
fields and their text, and return enough of each record to judge it and to address it.* They differ
only in where records come from and which clause is used. Fix those two contracts and everything
else is an extension point — commands become a record source rather than a search feature, SQL
becomes an engine over the same records, RAG becomes a record source plus a clause, and a
third-party engine becomes an implementation of selection.

The split that makes it small: **projection is a command** (it varies with the value type, an open
set) and **selection is a filter over the records it produces**. Phase 1 put selection on the store
trait; Phase 2 revision 2 reversed that — with records produced by commands, a trait method is a
push-down *optimization* rather than the mechanism, so it is deferred until a backend can exploit it.

### Load-bearing conclusions

1. **A search must never evaluate.** The asset layer can produce a value for a key only a recipe
   declares; a content search reaching through to that could recompute an entire corpus from one
   query.
2. **The store is not the whole corpus.** `AssetManager::get_asset_info` already resolves a key as
   live asset → store → recipe provider. A search seeing only the store answers a question nobody
   asked.
3. ~~**Selection belongs below the consumer.**~~ **Reversed by Phase 2 revision 2.** The original
   reasoning was that filtering in the caller is O(corpus) and forecloses any backend that could do
   better, so a command-only implementation "would ship faster and have to be undone". That
   mis-located the cost: producing the record stream is what reads the corpus, and the filter is
   cheap wherever it sits. A store method is a push-down optimization, addable later without
   changing a consumer. The consequence is recorded honestly — this work does **not** close
   `STORE-NO-CONTENT-OR-METADATA-SEARCH`, which asks for selection on the store.
4. **wasm decides the engine question before anything else does.** `liquers-web` is wasm32-only and
   browser search is essential, while the mature Rust full-text engines are server-oriented —
   Tantivy's own wasm RFC says so. The baseline must run everywhere; an engine can only ever be an
   optional override.
5. **The Whoosh prototype's mechanism is the thing to avoid, not the goal.** Indexing rode along
   inside a metadata-transformation hook, with a write side and no read side. The load-bearing
   defect generalizes: *a push hook makes correctness depend on delivery*, and every missed delivery
   is permanent and undetectable — which is why `reindex_store()` had to exist and why it can only
   answer "rebuild everything?" rather than "is it right?".
6. **Correctness comes from reconciliation; push is only a latency optimization.** An external
   system is a materialized view, and a set-diff of `(id, version)` streams tells it what it is
   missing — including deletions, the half push hooks usually get wrong. This is what makes one
   layer serve a search engine, a vector store, a RAG pipeline and an external SQL database alike:
   they differ only in what they *answer*.
7. **The layer needs almost no new vocabulary.** `Version` is already a content hash on every stored
   record; `DependencyRecord { key, version }` is already the shape of "what I observed";
   `register_version` → `expire_stale_dependents` is already the push cascade; `Expires` already
   declares how stale a view may be; `ExpirationMonitor` is already a worker of that shape. Exactly
   one new concept is required: an identity for the *projection rule*, so that changing a tokenizer
   or an embedding model invalidates an index rather than leaving it looking fresh.
8. **Three scales, deliberately distinct: a chunk is the unit of refresh, a batch the unit of
   memory, a record the unit of retrieval.** A record is derived and has no independent existence,
   so versioning one costs a full read of its source; a chunk is the smallest unit whose staleness is
   decidable from metadata alone. But the unit that is natural for dependencies — one parquet file —
   may be far too large to hold, so a chunk is *delivered* as batches. Partitioning by dependency and
   partitioning by size are different questions; one mechanism answers both, and batches never appear
   in a reconciliation diff.
9. **Identity is a pair, and the expensive half is derived.** A record is identified by its asset and
   an asset-dependent record id — a row number, a line, a pointer, or nothing when the asset *is* the
   record. Holding an evaluable query per row costs a multiple of the data it describes, so the asset
   is carried by the chunk and the locator query is constructed on demand, through `ActionRequest`
   rather than string templating. `Value::Object` is the right representation for the fields and the
   wrong one for the whole record: it cannot guarantee identity, cannot distinguish text from
   exact-match fields, and mixes provenance into what SQL would project.
10. **The record model has four consumers, of which search is one** — search, external sinks, SQL and
   serialization, since a chunk is a table and a stream is a table in parts. The search MVP needs
   only Level 0 (one record per asset, no ids, no batching), which must be the *degenerate case* of
   Level 1 rather than a second type. Whether Level 1 graduates to its own design is a decision to
   take deliberately.
11. **Key-prefix filtering carries more weight than metadata filtering.** Measured on this
   repository's 321 tracked documents, "what does this project know about expiration" gives 72 text
   hits; narrowing to `specs/issues/` — a key prefix, free, no metadata — gives 42; narrowing to
   still-open gives 23. Genre is mostly the folder and `area` overlaps what the text already found,
   so **lifecycle state is the only filter that genuinely needs metadata**, which narrows the
   `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` dependency to one filter rather than three. Text search
   is the primary act; filtering is a precision aid over its result.
12. **`status` means two unrelated things** — the asset lifecycle in `MetadataRecord`, the
   document's lifecycle in `specs/` front-matter — and a bare `status:draft` would resolve silently
   to either. Field resolution needs a namespace or a documented precedence.
13. **The level is cardinality; field provenance is a separate axis.** An earlier draft defined
   Level 0 as "one record per asset, fields from metadata", fusing two independent things. Extracting
   front-matter from Markdown yields one record per asset — Level 0 by cardinality — while taking its
   fields from content.
14. **The non-evaluation invariant needs a mechanism in the asset manager, not an argument about
   commands.** Two drafts looked at the wrong component — one made the projection the danger, the
   other argued it was safe because of how commands execute. The operation to prevent is *starting an
   asset*, and at HEAD `AssetManager::get_asset_info` starts one by accident: a live key is routed
   through `get`, which submits to the job queue when the asset is unfinished and cannot fast-track,
   so describing an asset in `Status::Recipe` runs it. Filed as
   `DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION` (P1). The non-triggering pieces already exist —
   `lookup_key_asset`, and `AssetRef`'s `status`, `poll_state`, `get_any_status`, `get_asset_info` —
   they are simply not recognised as the safe path. With that settled, inline projection is a
   secondary question and an allowed one: applying a command to a state creates no asset, and a text
   clause already reads the same bytes. Materializing a projection into metadata is an
   **optimization**, not a precondition — which is what this repository already does by hand with
   `specs/index.csv`. The residual hazard is a projection command reaching through its `Context`:
   `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`.
15. **Indexation may evaluate; search may not.** They are different acts with different budgets — a
   user's question with a latency budget, against a scheduled job whose scope and cost its owner
   accepted. That resolves the apparent conflict between "a search never evaluates" and "some
   documents must be produced at indexation time". **Metadata is always indexable**, so
   discoverability never requires evaluation; only *content* needs a policy, of which *when-ready* is
   the safe default and the one the MVP arrives at for free.
16. **Four classes of document, separated by whether a version is knowable without producing it.**
   *Stored* has a content hash. *Transient* — deterministic, cheap, deliberately not stored, the
   shape of a report rendered from precalculated data — has a version derived from its dependencies,
   so staleness is decidable without producing anything and producing it is unambiguously correct.
   *Ad-hoc query* — a non-keyed asset, a report generated on the fly — also has a version, derived
   offline from the plan's dependency and command versions, and it is where **command
   implementation versions become load-bearing**: with no stored content to compare, a changed
   command is the only thing that says the output would differ. *Volatile* has no version at all.
   All three of the latter need `produce`; only the volatile one cannot tell you whether it needs it.
17. **The partition is also the enumeration mechanism for non-keyed assets.** A keyed corpus is
   enumerable — that is what `listdir` is — and the space of queries is infinite, so a set of ad-hoc
   queries must be *declared*. The partition already is a list of `(chunk id, query, version)` and
   nothing requires a chunk query to be key-rooted, so the construct built for partial refresh turns
   out to be the enumeration mechanism too. This is also why record identity is a query rather than a
   key: `AssetInfo` already carries `query` and `key` independently, and the manager already holds
   query assets in their own map. Liquers cannot express the transient class today —
   `CommandMetadata.cache` is read by nothing, the macro cannot set it, and
   `PersistenceStatus::NotPersisted` means the write failed — and the workaround of declaring such an
   asset volatile throws away the version that made it tractable
   (`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`).
18. **Volatile assets need a second refresh regime.** They are never persistently ready, so
   *when-ready* never indexes their content at all, and they **never register a version** — version
   registration is gated on `Ready | Source | Override` (`assets.rs:5583`, `:5715`). So the
   `(id, version)` reconciliation diff has nothing to compare, and such entries must be refreshed on
   a schedule and reported as time-based rather than version-vouched. Volatility is knowable before
   evaluation (`plan.is_volatile`), so the policy is applicable without running anything.
19. **Only seven decisions are foundational.** The test is whether a thing can be added later
   without changing what Level 0 shipped. Seven cannot — the record's shape with the id field present
   but unused, identity as a pair, fields as a named `Value::Object`, a bounded opaque result rather
   than a `Vec`, the ordering promise, the text/field distinction, and non-exhaustive enums. Schema,
   chunks, batches, locator rules, streaming, projection identity and reconciliation are all
   additive. Milestones M0–M3 are the deliverable and are exactly what the agent memory MVP needs.
20. **Borrow tinysearch's data structure, not tinysearch.** It builds its index at build time and
   emits a compiled wasm module, which does not fit a corpus mutated at runtime. But a per-document
   word filter is a derived asset of *one* document, so it has no dependency fan-out — which is the
   problem that makes a monolithic derived index unattractive. In-tree indexes ride the asset layer
   because they *are* assets; external systems need reconciliation precisely because they cannot be.

## Phase 2 revision 2

The first Phase 2 draft was reviewed and substantially rewritten. Four corrections are worth keeping
in view, because two reverse Phase 1 decisions:

1. **The predicate is a pure filter.** `root` and `sources` left it. What is filtered is a *stream of
   records*, whose identity is the **query** that produces it — which also yields the two executions:
   evaluate the query and filter the stream, or hand the query to an engine that already processed
   it.
2. **`select` is dropped from `AsyncStore` and `AssetManager`** — reversing Phase 1 axis B2/B3. With
   records produced by commands it is a push-down optimization, not the mechanism. Nothing in the
   store trait changes, the conformance rule family disappears, and the honest cost is that
   `STORE-NO-CONTENT-OR-METADATA-SEARCH` is not closed by this work.
3. **A record stream — batches and chunks — is what `select` was standing in for**, and is now the
   core abstraction. It is **`futures::Stream`**, not a bespoke trait: `futures` is already a direct
   dependency of `liquers-core`, and `maybe_send.rs` already carries the per-target boxed-type
   pattern it needs (including the documented trap that `StreamExt::boxed()`, like
   `FutureExt::boxed()`, is always `Send`-boxed and therefore wrong on wasm). So `RecordBatchStream`
   is a type alias, `schema` moves onto the chunk descriptor where it belongs, and applying a
   predicate becomes a combinator. Defined in M0 with one trivial implementation, because a stream
   interface is F4 generalized and expensive to retrofit.
4. **Revision 3: the `Record` struct is gone too.** Singling out `text` matches no system this
   design integrates with — Tantivy and Lucene build a schema from fields with *options*, and
   privilege no text field. So identity, source and text are all **fields with roles**, rows are
   positional, and a per-batch **schema** owns names, types and roles. That also fixes a real
   inefficiency: a map per row re-creates every field name for every record, where a schema stores
   them once and lets a field name resolve to an index once per query. The structural guarantee
   Phase 1 asked for moves rather than disappearing — `RecordSchema::new` fails unless exactly one
   field has role `Id`. Cells are a `FieldValue` enum, measured at 24 bytes against JSON's 32:
   smaller *and* able to carry bytes, timestamps and vectors, the last being what RAG needs and JSON
   does worst. Not a type parameter — it would infect the stream, the predicate and `Value` itself,
   and Arrow, GlueSQL, Tantivy and Qdrant all chose a dynamic type enum for the same reason. The
   model is **Arrow-shaped but not Arrow-dependent**.
5. **Revision 4: the batch is columnar, in Arrow's memory layout.** Revision 3 stored rows row-major,
   justified by "the consumer is a predicate walking one row at a time" — which does not survive: a
   predicate over a column yields a boolean mask and clauses AND their masks, the polars and DuckDB
   shape, and faster. The decisive argument is different, though: of the three cheap routes out to
   pandas and polars, **two work only if the data is already in Arrow's layout** — the Arrow C Data
   Interface, and typed arrays viewing wasm memory. From `Vec<Row>` both need a full transpose and
   re-encode; laid out Arrow's way the hand-off is a pointer. Compactness follows: an `i64` column is
   8 bytes per value against `FieldValue`'s 24, and a null is one bit rather than a slot. The safety
   split is deliberate — core owns the layout, which is `Vec`s and bitmaps and adds no `unsafe` to a
   crate that has one occurrence in total, while the FFI belongs in `liquers-py` and Arrow IPC bytes
   in `liquers-lib`. No claim of full Arrow support: no nested `Struct`, no `Union`, no dictionary
   encoding, no 64-bit offsets. And it answers a requirement rather than a nicety — `liquers-web` is
   wasm32 and cannot bundle polars, so the columnar batch **is** that build's DataFrame.
6. **Revisions 5 and 6: the predicate is an expression in syntax, parsed by the command.**
   Revision 2's clause chain mixed building a record stream with progressively reducing it, and a
   pipeline is not an expression: it cannot express `OR` or grouping, never produces the predicate as
   a whole — so an external engine gets a sequence of steps to reverse-engineer — and is eager, which
   forecloses filter-then-verify and push-down. Revision 5 reached for **link parameters**
   (`~X~<query>~E`), which work but require a `Value::Predicate` variant. Revision 6 declines that:
   a **syntax string in one parameter** answers all three objections better — grouping lives in the
   grammar, the predicate travels as text an engine parses with the same parser, and it is parsed
   before the stream is touched — at no cost to the core value enum. The conditions under which the
   variant would earn its place are written down rather than guessed. The one honest cost is that
   every operator needs escaping, so a search URL is built by a UI or `ActionRequest`, never typed.
7. **`Hit` was faulty and is gone.** It embedded an `AssetInfo`, which assumes one record per asset —
   a CSV row has no `AssetInfo`, the file does. Asset description moves to a per-source table, and a
   hit is *retrievable*: `ChunkOrigin::chunk` re-produces the batch, `locator` addresses one record
   directly. Fields are `serde_json::Value`, not `liquers_core::value::Value`, which at 704 bytes was
   an outright bug in a document that cited `CORE-VALUE-ENUM-OVERSIZED` two sections earlier.

## Relationship to `agent-memory-mvp`

That design's Phase 1 open question 3 asks how far its MVP goes on search, noting a subtree scan is
"honest at ~300 documents and wrong at 100×". This design answers it: search is a capability with
a query surface, not a command private to `ns-mem`. The two designs share the non-evaluation
invariant and the "no identity, no ACL" exclusion (`CORE-SESSION-AND-KEY-ACL`).

## Scope boundaries

**Optional** — reachable later, not built now: ranked full text, semantic search and RAG, a
maintained index, any actual external sink, tags, facets and ranges, router fan-out, a dedicated
HTTP endpoint. `options-analysis.md` §5 records why each is excluded and what keeps it cheap.

**A separate task** — SQL. Considered here only where it intersects: an external SQL database is fed
and kept fresh by the same interoperability layer as any other external system
(`interoperability-layer.md` §6). The single requirement it places on this design is that records
carry named, typed fields, so that a SQL column and a search predicate's field are the same thing.

**Excluded outright** — identity and access control, consistent with `CORE-SESSION-AND-KEY-ACL`.

**Not search** — link traversal, dependency-graph queries and DataFrame filtering. Each has, or
deserves, its own mechanism.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Use cases](./use-cases.md)
- [Research questions](./research-questions.md)
- [Options analysis](./options-analysis.md)
- [Record model](../record-streams/record-model.md)
- [Indexation policy](./indexation-policy.md)
- [Interoperability layer](./interoperability-layer.md)
- [Roadmap](./roadmap.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
