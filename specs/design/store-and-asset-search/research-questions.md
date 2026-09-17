# Research questions

The nine questions raised against the first Phase 1 draft, answered. Evidence is from the codebase
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

## 3. Can we integrate a third-party engine instead of writing one? And what is wrong with the
Whoosh prototype?

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

**GlueSQL is the right shape for (2)** and it is genuinely additive rather than competitive. It is a
Rust SQL library — parser, execution layer, pluggable storage — whose custom backends implement
`Store` (SELECT) and optionally `StoreMut`, `AlterTable`, `Index` and `Transaction`. It supports
schemaless and semi-structured data (`MAP`, `LIST`) and can join schema'd against schemaless tables,
which is exactly the shape of a heterogeneous corpus.

The integration point is worth stating precisely, because the names collide: GlueSQL's `Store` is a
*rows-and-tables* trait, not Liquers' `AsyncStore`. The adapter exposes **a record set as a GlueSQL
table** — the same records the predicate filters. So:

- `search` answers "which entries match" — cheap, universal, works on wasm.
- `sql` answers "join, group, aggregate over these entries" — richer, optional, feature-gated.
- Both read the same records, so the SQL table's columns are the same fields the predicate tests.

That shared record is what makes SQL an *addition of an engine* rather than a second retrieval
system with its own view of the corpus.

> **Commits us to:** keeping fields *named and typed* on the record, since a SQL column and a
> predicate field must be the same thing. It does not commit us to building the adapter now.

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
