# Use cases — what "searchable" has to mean

Companion to [Phase 1](./phase1-high-level-design.md). Phase 1 states the delimitation; this
document is the survey it was drawn from, and the record of which use cases are **essential** and
which are **optional but must not be foreclosed**.

Two use cases are essential by instruction and everything else is measured against them:
**excellent support for the agent memory system** (§1) and **a simple search field for the user**
(§2). A design that serves those two well and admits the rest later beats a design that serves all
of them adequately.

---

## The classification

| Class | Meaning |
|---|---|
| **E** | Essential. The first version is not done without it. |
| **O** | Optional. Must be reachable later without breaking the contract; not built now. |
| **X** | Not search. Named here so it stops being proposed as search. |

---

## 1. Agent memory

| # | Use case | Class | Note |
|---|---|---|---|
| A1 | Find documents on a topic across a corpus (free text) | **E** | The base case. Almost always applicable, whatever the corpus is |
| A2 | Judge a hit without opening it | **E** | `title` + `description` + why it matched. This is what keeps context spend down |
| A3 | Filter by structured facts — `kind`, `status`, `area`, `priority`, type, error state | **E** | The corpus has a schema; ignoring it makes the agent read ten documents to find one |
| A4 | Discover what capabilities exist: which commands, what they do, what they take | **E** | An agent that cannot enumerate commands cannot compose queries. §5 |
| A5 | Discover what data exists under a prefix, with types | **E** | `listdir_asset_info` already does this; search is the filtered form of it |
| A6 | Budgeted retrieval — top *k*, a cap, and a truthful "there were more" | **E** | An agent's context is the scarce resource. An unbounded result set is a bug |
| A7 | Every hit is an address the agent can fetch, compose on, or cite | **E** | A hit that is not a query is a dead end |
| A8 | Re-find something the agent itself wrote | O | Needs a writable area; that is `agent-memory-mvp`'s scope, and it reads back through A1–A3 |
| A9 | Semantic recall — "things like this one", not "things containing this word" | O | §9. The contract must leave room for a similarity clause and a score |
| A10 | Follow a link: this issue's design folder, this design's issues | **X** | Front-matter traversal. A graph walk, not a predicate over a corpus |

## 2. A user's search field

| # | Use case | Class | Note |
|---|---|---|---|
| U1 | Type words into one box, get a list with title, type, icon, location | **E** | The named essential. One input, no syntax required |
| U2 | Narrow by type — "all images", "all dataframes" | **E** | A field predicate; free once A3 exists |
| U3 | Find by name or path fragment | **E** | Key substring or glob. The most common thing anyone actually types |
| U4 | Find broken or stale entries — status is error, expired | **E** | Falls out of A3; the operator case costs nothing extra |
| U5 | Scope the search to the directory being browsed | **E** | A root is part of every search anyway |
| U6 | A command palette: search commands, insert into the query | **E** | Same mechanism as A4, different front end |
| U7 | Narrow as you type, over a large store | O | Latency work: incremental, cancellable, index-backed. Design must not forbid it |
| U8 | Sort by relevance | O | §7/§9. Until ranking exists, order is unspecified |
| U9 | Ranges — modified this week, larger than 10 MB | O | A predicate clause; additive |

## 3. Structured and tabular data

| # | Use case | Class | Note |
|---|---|---|---|
| D1 | Find which stored table has a column, or which JSON/YAML has a key | O | Needs projection of structure into searchable fields. §4 |
| D2 | Query rows with SQL across stored tabular files | O | **A separate task.** It intersects here only through the feed: an external SQL database is fed and kept fresh exactly like a search engine or vector store. §4, and `interoperability-layer.md` §6 |
| D3 | Filter a DataFrame with the value type's own semantics | **X** | Already the `pl` namespace's job. Search should route to it, not reimplement it |
| D4 | Search inside configuration values | O | D1 with a different projection |

## 4. Operations

| # | Use case | Class | Note |
|---|---|---|---|
| O1 | Which assets failed, expired, or are stale | **E** | Same field predicate as U4 |
| O2 | What depends on this key | **X** | The dependency manager owns this. A graph query, not a search |
| O3 | Which recipes produce keys under here | O | A predicate over recipe records once recipes are a record source |
| O4 | Where is this command used | O | Requires indexing query text inside recipes; useful, not first |

## 5. Platform

| # | Use case | Class | Note |
|---|---|---|---|
| S1 | Search over HTTP for a non-Liquers client | **E** | Free if search is a command: `/q` already serves it |
| S2 | An MCP tool surface for agents | **E** | §6. Small fixed tool set over the query language |
| S3 | Search in the browser, over a browser store | **E** | `liquers-web` is wasm32-only. **This is the constraint that decides the engine question** |
| S4 | One search across several mounted stores | O | Router fan-out. Specify, build later |
| S5 | Let a capable backend do the filtering | O | The reason selection belongs on a trait rather than in a command |
| S6 | Plug in an external search engine, vector store or RAG pipeline without tying to one | **E** | The *layer* is essential; shipping a sink is not. `interoperability-layer.md` |
| S7 | Minimal built-in search that depends on nothing external | **E** | At least metadata. The floor that makes every other option optional |

---

## What the classification implies

**Four conclusions the option analysis has to respect.**

1. **Full text and field predicates are one feature, not two.** A1 and A3 are both essential and
   every consumer wants them together — "issues about expiration that are still open" is one
   question. A design that ships free text now and fields later ships half of both essentials.

2. **Command discovery is essential, and it is not a second search system.** A4 and U6 are the same
   need from two front ends. The cheap way to satisfy them is to make commands a *source of
   searchable records* rather than to build a command-search feature. §5 of the research questions
   works this through.

3. **S3 constrains the engine before any other consideration does.** `liquers-web` is wasm32-only,
   and the mature Rust full-text engines are server-oriented — Tantivy's own wasm RFC records that
   it was "conceived as a library for developing server-side indexers" and never committed to wasm.
   So the *baseline* implementation must be one that runs anywhere, and a third-party engine can
   only ever be an optional override. That is a constraint, not a preference.

4. **Three things are not search and should stay out.** Link traversal (A10), dependency queries
   (O2) and DataFrame filtering (D3) each already have, or deserve, their own mechanism. Folding
   them in would make the predicate language grow without making any essential use case better.

5. **S6 and S7 are a pair, and the pairing is the point.** A minimal built-in that depends on
   nothing (S7) is what allows every engine to be optional; a layer that plugs any engine in (S6) is
   what stops the minimal one from becoming a ceiling. Either alone produces a bad design — a
   built-in engine with no escape hatch, or a framework that cannot search without a service.

---

## The common denominator

Reading the essential rows together, every one of them is the same operation:

> **Select records from a set, by a predicate over their fields and their text, and return enough
> of each record to judge it and to address it.**

The rows differ only in **where the records come from** — a store subtree, an asset listing, the
command registry, later a table's rows or a vector index — and in **which clause of the predicate
is used**. That is the whole basis of the recommended design: fix the record and the predicate,
and let sources and clauses be added without changing either.
