# Interoperability with external index-like systems

Companion to [Phase 1](./phase1-high-level-design.md). This is the design's most consequential
question and it has its own document: **can one mechanism serve an external search engine, a vector
database, a RAG pipeline and an external SQL database — and can it handle updates and expiration?**

The answer is yes, and the reason is that all four are the same thing wearing different clothes:
**a materialized view of a Liquers corpus, living outside Liquers, answering questions Liquers
cannot answer cheaply itself.** Each has exactly three obligations — be fed, answer, and stay
consistent — and only the second differs between them.

The prior art is the Python LiQuer indexer (`liquer/indexer.py` + `ext/lq_whoosh.py`), which the
brief describes as working but not feeling right. §1 says precisely why, because the reason
generalizes into the rule the rest of this document is built on.

---

## 1. Why a push hook is the wrong foundation

The prototype registers priority-ordered `Indexer` callables invoked during metadata generation;
`AddToWhooshIndex` writes a Whoosh document as a side effect and returns the metadata unchanged.

**The load-bearing defect is not any of its details. It is that correctness depends on delivery.**
Every one of these leaves the index permanently and undetectably wrong:

- a write through a path that does not fire the hook;
- a crash between the write and the hook;
- a sink that was offline, restarting, or slow;
- a corpus that existed before the sink did;
- a sink rebuilt, moved, or shared between two processes.

`reindex_store()` exists because of this, and it is an admission rather than a fix: it is
all-or-nothing, so the only questions you can ask are "rebuild everything?" — never "is it right?".

> **The rule this yields:** *correctness comes from reconciliation; push is only a latency
> optimization.* A lost notification must cost seconds, never consistency. Everything below follows
> from that one sentence.

Four further defects of the prototype are worth keeping on the list because the replacement must
avoid them too: indexing rode inside a *metadata-transformation* interface (two unrelated jobs, one
hook); there was a write side and no read side, so nothing in the store abstraction said "this is
searchable"; `data` was passed in and discarded, so "full text" was really title-and-description;
and results left the type system as hand-built dicts.

---

## 2. What the four systems have in common

| System | What it is fed | What it answers | Needs freshness? |
|---|---|---|---|
| External search engine (Tantivy, Meilisearch, Elasticsearch) | text + fields per record | text and field predicates, ranked | yes |
| Vector database (Qdrant, LanceDB, usearch) | chunks + embedding vectors | nearest-*k* similarity | yes |
| RAG pipeline | chunks + vectors, sometimes summaries | retrieve-then-read | yes |
| External SQL database | records as rows | SQL: joins, groups, aggregates | yes |

Identical columns except the third. That is the whole argument for one mechanism: **the feed and
the freshness are common; only the query is specific.** So the layer standardizes the first two and
leaves the third to each system's own contract.

It is also why the SQL task can be split off cleanly (§6).

---

## 3. The contract: three roles

### Role 1 — the feed (Liquers side)

For a scope (a root key, optionally narrowed):

- **`versions(scope)` → `(RecordId, Version)`** — cheap, metadata only, no data reads.
- **`fetch(ids)` → `Record`** — the full projection, for the ids the sink actually needs.

Two calls rather than one, deliberately: the diff is cheap and proportional to the corpus in
*metadata*, while the fetch is proportional to what actually **changed**.

**Liquers already has both.** `MetadataRecord.version` is a content hash computed at save time
(`liquers-core/src/metadata.rs:961`), and `listdir_asset_info` returns per-entry descriptions
without reading data. Nothing new is required to make a corpus feedable.

### Role 2 — the sink (external side)

- **`observed(scope)` → `(RecordId, Version)`** — what it currently holds, or a digest of it (§4).
- **`apply(upserts, deletions)`** — idempotent, so a repeated or partial sync is safe.
- **`freshness()`** — what it reflects, and how confident it is.

Reconciliation is then a set-diff of two version streams: upsert where the version differs or the
id is new; delete where the sink holds an id the feed no longer offers. **Deletion detection is the
half a push hook usually gets wrong**, and a diff gets it right for free.

### Role 3 — the selector (query side)

A sink that can answer implements the same selection contract as everything else, so it is readable
*through* the abstraction rather than beside it — the exact gap that made the prototype feel
bolted on. A sink that cannot answer search queries (a SQL mirror) simply does not implement it and
is never consulted by a search.

---

## 4. Making reconciliation cheap

`observed()` over a large corpus is itself O(corpus), which would put the cost back where it was
taken from. Two standard answers, in the order they should be reached for:

1. **Scope it.** Reconcile a subtree, not a store. Most corpora change in a few places.
2. **Hierarchical digests.** A directory's digest is a hash over its children's `(id, version)`
   pairs; compare top-down and descend only where digests differ. This is the Merkle-tree trick,
   and **Liquers' directory structure already supplies the tree** — so an unchanged subtree costs
   one comparison. Worth naming now as the scaling answer, not worth building first.

At the scale of the essential use cases — hundreds to low thousands of records — plain scoped
reconciliation is adequate and the digest layer is premature.

---

## 5. How updates and expiration integrate

This is what makes the layer Liquers-native rather than a generic change-data-capture design: every
concept it needs already exists, under a name the codebase already uses.

| Need | Existing mechanism | Where |
|---|---|---|
| A freshness token per record | `Version` — a content hash, already on every stored record | `metadata.rs:961` |
| "What I observed, and at which version" | `DependencyRecord { key, version }` — exactly the shape a sink's observed set has | `metadata.rs:285` |
| Being told a source changed | `register_version` → `expire_stale_dependents` cascade | `dependencies.rs:158` |
| A change notification channel | `AssetNotificationMessage` on a watch channel, including `ValueProduced` and `Expired` | `assets.rs:368` |
| Declaring how stale a view may be | `Expires` — `Never`, `Immediately`, `InDuration`, `AtTimeOfDay`, `OnDayOfWeek` | `expiration.rs:11` |
| A background worker holding timers | `ExpirationMonitor`, already a running service of exactly this shape | `assets.rs:4594` |

So a sink is, formally, **a dependent with many dependencies** — and the mechanism that expires
stale dependents is already built and already tested. The layer's job is to give an external system
a seat at a table the dependency manager already runs, not to build a second table.

`Expires` is the piece that turns an operational argument into a declaration: a sink declaring
`InDuration(5 min)` means "reconcile at most every five minutes, and a search may see a
five-minute-old view"; `Immediately` means "reconcile before answering". The staleness budget
becomes configuration instead of folklore.

**Two honest caveats, both consequences of the sink being outside the process:**

1. The dependency manager is in-process and in-memory. A sink may be in another process, on another
   machine, down, or shared by several Liquers instances. So the cascade is **the fast path**, and
   reconciliation is **the guarantee**. This is §1's rule restated where it bites.
2. A sink watching a directory for *additions* depends on a directory-listing dependency — which is
   recorded and then silently dropped today
   (`DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`). That blocks the push path only.
   The pull path is unaffected, which is a third argument for making pull the guarantee.

---

## 6. Where SQL intersects, and where it does not

An external SQL database is a sink whose projection is **rows**. Everything in §3–§5 applies
unchanged: version diffing, reconciliation, deletion detection, staleness declaration.

What belongs to the separate SQL task (`NO-SQL-QUERY-CAPABILITY-OVER-STORED-AND-DERIVED-DATA`) and
is **not** settled here: the query language, join and
aggregation semantics, schema mapping and evolution, type coercion, and write-back. The split is
clean because the layer standardizes the feed and the freshness and leaves the query to each
system.

One design consequence must be honored now, though, or the split will not hold: **records carry
named, typed fields**. A SQL column and a search predicate's field have to be the same thing, or
the two tasks will grow two incompatible views of the same corpus. That single requirement is the
whole of what the SQL task imposes on this one.

The same reading applies to RAG: chunking and embedding are commands producing derived assets, the
vector store is a sink, nearest-*k* is a query clause. The valuable part is the freshness — knowing
exactly what to re-embed when a document changes is the hard problem of production RAG, and it is
the one thing the asset layer already solves.

---

## 7. What this layer does not solve

Naming these now prevents them being discovered as surprises:

- **No transactionality across Liquers and the sink.** Reconciliation is eventual; a search may see
  a view that is behind, and the contract says so rather than pretending otherwise.
- **Tombstones.** A sink offline while a record is deleted learns of it at the next full-scope
  diff, not from a notification. Scoped reconciliation bounds how long that takes.
- **Multiple writers to one sink.** Two Liquers instances reconciling the same sink is out of
  scope; it needs ownership or leasing.
- **Authorization at the sink.** The sink sees what it is fed. Consistent with the design's
  no-identity exclusion.
- **Projection versioning.** If the *projection rule* changes (a new field, a different tokenizer, a
  new embedding model), every record's record changed although no document did. The layer needs a
  projection identity that participates in the diff — otherwise a model upgrade silently leaves a
  stale index looking fresh. **This is the one genuinely new concept the layer requires**, and
  Phase 2 must design it.

---

## 8. Recommendation

1. **Build the feed and the reconciliation contract; do not build a hook registry.** The feed is
   nearly free — `versions` and `fetch` over what `listdir_asset_info` and the store already
   provide.
2. **Make push an optimization with no correctness role.** Wire it to the existing notification
   channel when convenient; never let a sink's correctness depend on receiving anything.
3. **Express staleness with `Expires`**, so the policy is declared per sink rather than hardcoded.
4. **Keep the sink's query side behind the ordinary selection contract**, so an external engine is
   an implementation rather than an appendage.
5. **Do not ship a sink in the first version.** Ship the contract with an in-memory test sink that
   proves reconciliation, and let the first real sink be written against a contract that already
   has a test double. A trait with one implementation is a guess; a trait with a real
   implementation and a test double is a design.
6. **Design projection identity now** (§7, last bullet). It is cheap while the record type is being
   defined and expensive afterwards.
