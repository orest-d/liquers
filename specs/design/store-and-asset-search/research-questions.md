# Research questions

The questions raised against the Phase 1 drafts, answered — nine from the first round, two more
(§10, §11) from the scope slice that followed. Evidence is from the codebase
at HEAD (2026-09-17), from the Python LiQuer prototype, and from the named external projects.

Each answer ends with **what it commits us to** — the part that has to survive into Phase 2 — kept
separate from the part that is merely a preference.

---

## 1. Store/asset-manager methods, or commands?

**Both, split by what varies.** The question assumes one has to win; the two halves of a search
vary along different axes, and putting each where it varies is what makes the design small.

| Half | Varies with | Therefore |
|---|---|---|
| **Projection** — turning a thing into something searchable: what its text is, what its fields are | the **value type** and the application | a **command**. Open set, non-standardizable, already dispatched on type |
| **Selection** — walking a candidate set and applying a predicate | the **backend** | a **trait method**. Closed set, centralizable, optimizable, and the only place an index can live |

A store is the only component that sees every write, so it is the only component that can maintain
an index without a synchronization problem (§3). A command is the only place that can know how a
`PolarsDataFrame`, a custom `ExtValue` or a foreign JavaScript value turns into text, because the
set of value types is open by design.

Putting projection in the store would require the store to understand every value type — it cannot,
it holds bytes. Putting selection in a command forecloses every backend optimization and lands the
retrieval logic in each consumer, which is the complaint already on file as
`STORE-NO-CONTENT-OR-METADATA-SEARCH`.

The two meet at one type: a **record**. A projection command produces records; a selection method
filters them.

> **Commits us to:** a record type in `liquers-core` that both halves speak; selection as a trait
> method with a default implementation; projection as commands. It does *not* commit us to
> projecting through a command on the hot path — a store entry's record comes from its metadata
> directly, and a projection command is needed only for values whose text is not their bytes.

---

## 2. What should the search capability be able to do?

Four levels, each strictly more formal than the last, and — this is the point — **all four are
predicates over the same record**.

| Level | Clause | Applicability | Class |
|---|---|---|---|
| **L1** Free text | substring or term match over a record's text | almost universal — anything with a textual rendering | essential |
| **L2** Fields | equality, prefix, set membership, later ranges, over named typed fields | anything with metadata; today the `MetadataRecord`/`AssetInfo` fields, later tags | essential |
| **L3** Structured | SQL or a value type's own query semantics, over records as rows | only for values with structure | optional |
| **L4** Similarity | nearest-*k* over a vector field | only where embeddings exist | optional |

L1 and L2 are one feature (see `use-cases.md` §"What the classification implies"). L3 and L4 are
**additional clause kinds and additional record sources**, not a different system — which is what
keeps them cheap.

Tags deserve a specific note: they are the obvious L2 field and Liquers has nowhere to put them.
`MetadataRecord` is a closed struct (`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`). The design must
therefore define field predicates over *fields generally*, resolved from whatever the record
carries, so that tags become a field the day that issue is fixed rather than a change to search.

> **Commits us to:** a predicate that is an extensible *set of clauses* rather than a fixed
> argument list, and field lookup by name against a record rather than against `MetadataRecord`'s
> struct fields.

---

## 3. Third-party engine instead of our own — and what is wrong with the Whoosh prototype?

**Yes — and the prototype shows exactly which part to keep and which to drop.**

The Python prototype (`liquer/ext/lq_whoosh.py` with `liquer/indexer.py`) works like this: a global
`IndexerRegistry` holds priority-ordered `Indexer` objects; each is a callable
`__call__(key, query, data, metadata) -> metadata` invoked during metadata generation;
`AddToWhooshIndex` ignores `data`, pulls `title`, `description`, `characteristics.description` and
`type_identifier` out of `metadata`, writes them into a Whoosh index, and returns the metadata
unchanged. `search()` runs a Whoosh query and returns a list of dicts with a hand-built `link`.
`reindex_store()` walks every key and re-adds it.

It works. Five specific things are wrong with it, and the diffuse "doesn't feel right" is their sum:

1. **Indexing is a side effect of a metadata filter.** The interface's contract is "return valid
   metadata"; indexing rides along. Two unrelated jobs — enriching metadata, populating an external
   index — share one hook, so neither can be reasoned about alone.
2. **There is a write side and no read side.** Nothing in the store abstraction says "this corpus
   is searchable". Whoosh becomes the only thing that can answer, out of band, and no contract
   relates the index to the store. The existence of `reindex_store()` *is* the admission that they
   drift.
3. **It indexes metadata while claiming full text.** `data` is passed in and discarded. What the
   prototype actually offers is search over titles and descriptions.
4. **Results leave the type system.** A list of dicts with a hand-built link is not a value, so it
   cannot be cached, composed, re-served, or rendered by anything generic.
5. **Freshness is imperative, in a system whose whole thesis is that it should not be.** Liquers'
   distinguishing property is that a derived thing recomputes when its sources change, by content
   hash. The indexer opts out of that and substitutes "hope every write path fires the event". It
   is a cache without a cache's invalidation contract — and *that* is the part that feels wrong,
   because it contradicts the thing the framework is for.

The constructive version keeps the pluggability and moves the ownership:

- **Keep:** a third-party engine is a legitimate implementation. Do not write an engine.
- **Drop:** the global event hook. Replace it with a store **decorator** — `IndexedStore<S>` wrapping
  any `AsyncStore`, maintaining its index on the writes it performs and declaring the selection
  capability. The component that owns the invariant is the component that can enforce it, and a
  decorator is already an established shape here (`AsyncStoreRouter`).
- **Constraint:** the decorator is optional, because the baseline must run on wasm (§ below), and
  because a store whose backend is written by someone else cannot see those writes and must fall
  back to scanning.

On engine choice: **Tantivy** is the serious Rust candidate — Lucene-class, BM25, used by ParadeDB,
Quickwit and Turso. It is also explicitly server-oriented; its own wasm RFC records that it "was
originally conceived as a library for developing server-side indexers" and that committing to wasm
would affect feature development and style. Since `liquers-web` is wasm32-only and browser search is
an essential use case (`use-cases.md` S3), Tantivy can be an **optional feature behind the
selection trait** and never the baseline.

> **Commits us to:** a read-side capability on the store trait, so an engine is an implementation of
> something rather than a thing bolted beside everything. And to *not* reproducing the indexer-hook
> mechanism.

---

## 4. Full text only, or query languages — and where does GlueSQL fit?

**Do not build a query language. Build a predicate, and let other languages compile to it or run
beside it over the same records.**

Three distinct things get conflated under "query language":

1. **A search syntax** for humans typing in a box — `"exact phrase" -excluded kind:issue`. This is a
   *front end*, a parser that produces a predicate. §7.
2. **A structured query language** — SQL. This is an *engine*, not a front end: it joins, groups and
   projects, which no predicate over a flat record set will ever do.
3. **A value type's own semantics** — Polars expressions over a DataFrame. Already the `pl`
   namespace's job, and search should route to it, never reimplement it.

**SQL is now a separate task**, filed as `NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`,
and is not designed here. What remains in scope is the single
requirement it places on this design, and the observation that it needs no separate integration
mechanism.

**GlueSQL is the right shape for (2)** when that task comes. It is a Rust SQL library — parser,
execution layer, pluggable storage — whose custom backends implement `Store` (SELECT) and
optionally `StoreMut`, `AlterTable`, `Index` and `Transaction`. It supports schemaless and
semi-structured data (`MAP`, `LIST`) and can join schema'd against schemaless tables, which is
exactly the shape of a heterogeneous corpus. The names collide and it matters: GlueSQL's `Store` is
a *rows-and-tables* trait, not Liquers' `AsyncStore`. The adapter exposes **a record set as a
GlueSQL table**.

**Where it intersects this design.** An *external* SQL database is fed and kept fresh exactly like
an external search engine or vector store — same version diff, same reconciliation, same staleness
declaration. That is [`interoperability-layer.md`](./interoperability-layer.md) §6, and it is why
splitting SQL off is clean: the layer standardizes the feed and the freshness; each system keeps its
own query contract.

- `search` answers "which entries match" — cheap, universal, works on wasm.
- `sql` answers "join, group, aggregate" — a different engine, a separate task.
- Both read the same records, so a SQL column and a predicate field are the same thing.

> **Commits us to exactly one thing:** records carry **named, typed fields**. Violate that and the
> two tasks grow two incompatible views of one corpus. Nothing else about SQL is decided here.

---

## 5. Command discovery

Commands are already data, and already partly addressable:

- `CommandMetadata` carries `realm`, `namespace`, `name`, `label`, `doc`, `module`, `arguments`
  (each an `ArgumentInfo`), `presets`, `next`, `filename`, `cache`, `volatile`, `is_async`,
  `expires`, `payload_required`, `definition`, and two versions
  (`liquers-core/src/command_metadata.rs:944`).
- `Value::CommandMetadata(CommandMetadata)` is a core value variant, so a command's description
  *is* already a first-class value.
- `ns-dep/command_metadata-{realm}-{namespace}-{name}` returns it, and `commands_doc` renders
  markdown for a namespace (`liquers-lib/src/commands.rs:278-296`).

What is missing is exactly one thing: **commands are not enumerable as a set**. The only convention
addressing a command as a query is the dependency notation, which names one command at a time, and
`commands_doc` renders prose rather than records.

Two options, and the second is clearly better:

- **A command-search feature.** A `search_commands` command with its own matching. Rejected: a
  second search implementation, diverging from the first the moment either changes.
- **Commands as a record source.** The registry projects to records — id = the command's query
  address, text = `label` + `doc` + argument docs, fields = `namespace`, `realm`, `name`,
  `is_async`, `volatile`, argument types. The *same* predicate then answers "which commands mention
  resize" with no new matching code at all.

There is an especially tidy form of the second: **a read-only virtual store over the registry**,
mounted at a key. Then commands are enumerable by `listdir`, describable by `get_asset_info`, and
searchable by the generic selection with **nothing command-specific in the search path at all**. It
also gives commands a stable key address, which is the convention the question notes is missing.
The cost is a virtual store and a decision about what a command's "data" is (its metadata as JSON is
the obvious answer).

> **Commits us to:** a record whose identity is a *query*, not a key — because a command's address is
> not a store key. Everything else here is an option Phase 2 can pick between.

---

## 6. What does an agent need, and what would MCP need?

From `use-cases.md` §1, six properties, in the order they bite:

1. **A small, fixed tool surface.** Composition belongs to the query language; an MCP server with
   one tool per command is a several-hundred-entry manifest that no agent reads well. Four tools —
   *search*, *read*, *list*, *describe* — plus the query language covers it. The wide surface is a
   cost, not a feature: an MCP tool manifest is the agent's attack surface as well as its menu.
2. **Tiered results.** Title and description first, body only on request. `AssetInfo` already
   carries both, which is why the memory design calls them L0 and L1.
3. **A reason for every hit.** Which field matched and a short excerpt. An agent given bare keys
   spends its context opening documents to find out which one it meant — the exact cost search is
   supposed to remove.
4. **Addressability.** Every hit is a query: fetchable, composable, citable, and stable enough to
   put in a note. This maps cleanly onto MCP resources — a Liquers key is a resource URI, and a
   parameterized query is a resource template.
5. **Budget honesty.** A limit, and a truthful signal that there were more. A silently truncated
   result set makes an agent conclude something does not exist.
6. **Vocabulary discovery.** What fields can I filter on, what values do they take, what commands
   exist. Without this an agent guesses field names. §5 covers commands; facet-value listing is the
   optional half.

> **Commits us to:** the match record (3), the limit-and-truncation signal (5), and a hit identity
> that is a query (4). The MCP server itself is `agent-memory-mvp`'s deliverable, not this design's.

---

## 7. Should free-text search have a syntax? Is there a standard?

**A tiny one, chosen to be a subset of what people already expect.**

There is no usable formal standard. The candidates:

| Syntax | Status | Verdict |
|---|---|---|
| **Lucene / Solr standard query parser** | De facto industry standard; `field:value`, `AND/OR/NOT`, `"phrases"`, `~fuzzy`, `^boost`, ranges, grouping | The vocabulary to borrow from. Also "very intolerant of syntax errors" by Solr's own account — adopting it whole means adopting its error behaviour |
| **CQL / SRU** (Library of Congress, from Z39.50) | A real standard, designed for human readability | Almost no one outside libraries knows it. Standard-compliance buys nothing here |
| **Google operators** | Informal but universally known: quotes, `-` exclusion, `site:`-style qualifiers | What a user will actually try without reading anything |
| **GitHub qualifiers** (`is:open`, `in:title`) | Informal, very widely known among this project's users | Same shape as `field:value` |

The intersection of Google, GitHub and Lucene is small and unsurprising, which is the whole
argument for it:

```
expiration safety          two terms, implicit AND
"expiration safety"        a phrase
-expired                   exclude
kind:issue status:draft    field predicates
```

Explicitly not in a first version: boolean grouping with parentheses, `OR`, wildcards, fuzzy `~`,
boosts, ranges. Each is easy to add to a parser and hard to push down to a backend, and none is
needed by an essential use case.

**One Liquers-specific hazard, which is not optional to solve.** The operator characters collide
with the query language's own: `-` separates action parameters, `~` is the escape character, `/`
separates segments. A user typing `-expired` into a search box is fine — the *UI* builds the query
and `ActionRequest::encode` escapes the whole string — but a hand-written URL becomes
`search-~_expired`, and a hand-written phrase search becomes `search-~"expiration~.safety~"`. This
is legible only through a builder. It is a strong argument for the search syntax living inside one
escaped string parameter, and for the documentation to show the builder rather than the URL.

> **Commits us to:** the syntax being a *front end* that compiles to the predicate, never the
> predicate's own form — so that a UI, MCP or HTTP caller can supply a structured predicate and skip
> parsing entirely.

---

## 8. RAG: should embeddings live in metadata?

**No — vectors do not belong in `MetadataRecord`, and the reason is concrete.**

1. **Size.** A single embedding is 384–1536 floats: 1.5–6 KB. Metadata travels with
   `listdir_asset_info`, which is the cheap-directory-description call the whole tiered-retrieval
   story depends on. Putting a vector in it makes a 300-entry listing carry ~1 MB of floats and
   turns the cheapest call in the system into one of the most expensive.
2. **A vector is meaningless alone.** It is only interpretable together with the model identity,
   its version, the dimension, the normalization and the chunking that produced it. A bare vector
   field invites silently comparing embeddings from two different models.
3. **There is more than one.** "Multiple vectors" is the normal case — one per chunk — so the
   natural cardinality is a collection per document, not a field on the document.

The shape that fits Liquers, and it fits unusually well:

- **Chunking is a command.** Document → chunks.
- **Embedding is a command.** Chunk → vector value.
- **The vectors are a derived asset** of the document, so the asset layer re-embeds when the
  document's content hash changes. *This is the hard part of every production RAG pipeline* —
  knowing what to re-embed — and it is the one thing Liquers already does. It is the strongest
  strategic argument for building RAG on this stack at all.
- **The vector index is a store or a derived asset**, behind the same selection capability.
- **Metadata carries a reference, not the payload**: which embeddings exist, under which model, at
  what dimension. That is precisely what `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` would enable.

> **Commits us to:** a result carrying a **score** field from the first version (unset until a
> ranking clause exists), and a predicate that can grow a similarity clause. Nothing else.

---

## 9. Semantic search in general

Semantic search is §8's machinery plus a ranking contract, and the ranking contract is the part that
affects this design now.

Boolean matching (L1/L2) promises no order. Similarity and BM25 both produce an **ordered, scored**
result, and a consumer that has learned to trust order will break if a later version introduces it
unannounced. So the contract has to say, from the first version: *results are unordered unless a
scoring clause was used; when one was, they are ordered by descending score and each hit carries it.*

The second thing worth settling early is that semantic and lexical search are not alternatives.
Every serious retrieval system runs both and merges (hybrid retrieval), because embeddings miss
exact identifiers and lexical search misses paraphrase. A predicate model that can hold a text
clause and a similarity clause **in the same predicate** gets hybrid retrieval as a consequence of
its shape. A design that treats semantic search as a separate command has to bolt merging on later.

> **Commits us to:** the ordering promise stated above, the score on the hit, and clauses being
> composable within one predicate rather than being alternative entry points.


---

## 10. One interoperability layer for search engines, vector stores, RAG and external SQL?

**Yes, and it should not be a hook system.** The analysis has its own document:
[`interoperability-layer.md`](./interoperability-layer.md). The summary:

All four are the same thing — a materialized view of a Liquers corpus, living outside Liquers,
answering what Liquers cannot answer cheaply. Each has three obligations: be fed, answer, stay
consistent. **Only the second differs between them**, so a layer that standardizes the feed and the
freshness serves all four and leaves each its own query contract.

The prototype's defect generalizes into the governing rule. A push hook makes correctness depend on
delivery, and every missed delivery is permanent and undetectable — which is why `reindex_store()`
had to exist and why it can only answer "rebuild everything?" rather than "is it right?".

> **Correctness comes from reconciliation; push is only a latency optimization.**

Reconciliation is a set-diff of two `(id, version)` streams — what the corpus has against what the
sink holds — and it gets deletion detection for free, which is the half push hooks usually get
wrong.

The part that makes it Liquers-native rather than generic change-data-capture is that **every
concept it needs already exists**: `Version` is the freshness token, `DependencyRecord { key,
version }` is already the shape of "what I observed", `register_version` → `expire_stale_dependents`
is already the push cascade, `AssetNotificationMessage` is already the channel, `Expires` already
expresses how stale a view may be, and `ExpirationMonitor` is already a background worker of exactly
this shape. A sink is formally a dependent with many dependencies. The layer gives an external
system a seat at a table the dependency manager already runs.

Two caveats are load-bearing: the dependency manager is in-process, so an out-of-process sink can
never rely on the cascade; and a sink watching a directory for *additions* depends on a
directory-listing dependency that is recorded and silently dropped today
(`DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`). Both argue the same way: pull is
the guarantee.

One genuinely new concept is required — **projection identity**. If the projection *rule* changes (a
new field, a different tokenizer, a new embedding model), every record changed although no document
did. Without an identity for the rule participating in the diff, a model upgrade leaves a stale
index looking fresh.

> **Commits us to:** a feed of `(id, version)` plus `fetch`; reconciliation as the guarantee;
> `Expires` as the staleness declaration; the sink's query side behind the ordinary selection
> contract; and projection identity designed while the record type is.

---

## 11. tinysearch: integrate it, or write a minimal engine?

**Borrow its data structure; do not integrate the tool.** And the reason to borrow *that* structure
in particular is much better than "it is small".

What tinysearch is, from its own documentation: the index is built **at build time** from a JSON
file; the output artifact is a **compiled WebAssembly module**; storage is either a sorted vocabulary
with exact posting lists or, optionally, **Xor8 filters per article**; roughly **2 kB per article**
uncompressed; prefix matching only from three characters and, with Xor8, only in titles; **no
ranking**; recommended for small- to medium-size sites.

Three of those rule out integration outright: a build-time generator does not fit a corpus mutated
at runtime, a compiled wasm artifact is not a library that can index in-process, and without ranking
or positions we would be adopting the algorithm regardless.

**But the per-document filter is the right idea, for a reason specific to Liquers.** A monolithic
inverted index over a corpus is one asset depending on every document — the fan-out problem that
makes the derived-index route unattractive. A **per-document word filter is a derived asset of
exactly one document**, so the existing dependency machinery invalidates exactly one filter per
change, with no fan-out at all. Search becomes two stages:

1. **Filter stage** — load the small per-document filters and test the query's terms. Reduces
   candidates from the whole corpus to hits plus a bounded false-positive rate.
2. **Verify stage** — read content only for the survivors, confirm the match, and extract the
   snippet.

The verification is not wasted work: the snippet has to come from the content anyway
(`use-cases.md` A2), so stage 2 is work the search already owed.

The error characteristics are exactly right for this. Bloom and xor filters have **false positives
and no false negatives**, so stage 2 removes every error the filter can make. Phrase queries work
too: stage 1 tests membership of each word, stage 2 checks adjacency on the survivors.

Honest limits, which are also the boundary where an external engine takes over: no positions, so no
ranking and no phrase *scoring*; filter scanning is still O(corpus) with a tiny constant — at ~600
bytes per document, hundreds of documents cost a scan of a few hundred kilobytes and a hundred
thousand documents cost tens of megabytes, at which point this stops being the right answer.

> **Commits us to nothing in the first version.** The floor is a metadata and content **scan** — no
> index, no dependency, correct everywhere including wasm. The filter index is the designed second
> step, and it needs no interoperability layer at all, because an in-tree index *is* an asset. That
> contrast is worth keeping: **in-tree indexes ride the asset layer; external systems need §10's
> reconciliation precisely because they cannot be assets.**
