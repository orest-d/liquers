# Options analysis — searchable stores and assets

Companion to [Phase 1](./phase1-high-level-design.md), which states the delimitation. The use-case
survey is in [`use-cases.md`](./use-cases.md); the nine research questions are answered in
[`research-questions.md`](./research-questions.md). **This document is the design analysis**: the
ground truth it rests on, the unifying model, the decision axes, and the recommended combination.

It names no types, signatures or modules — that is Phase 2.

---

## 0. Ground truth at HEAD

Verified against the code on 2026-09-17, because an options analysis built on a wrong picture of
what exists is worse than none.

| Fact | Where |
|---|---|
| `AsyncStore` can enumerate (`keys`, `listdir`, `listdir_keys`, `listdir_keys_deep`, `listdir_asset_info`) and fetch (`get`, `get_metadata`, `get_asset_info`). It has **no** selection method. | `liquers-core/src/store.rs` |
| `AssetManager` resolves a key as live asset → store → recipe provider. No enumeration of *live* assets, no selection. | `liquers-core/src/assets.rs:3795` |
| `AssetInfo` already carries `title`, `description`, `type_identifier`, `media_type`, `data_format`, `file_size`, `status`, `updated`, `unicode_icon`, `is_dir`, `is_error`, `is_volatile`. | `liquers-core/src/metadata.rs:678` |
| `Value::AssetInfo(Vec<AssetInfo>)` and `Value::CommandMetadata(CommandMetadata)` are already core value variants. A command's description is already a first-class value. | `liquers-core/src/value.rs:33,35` |
| `CommandMetadata` carries `namespace`, `name`, `label`, `doc`, `arguments`, `presets`, `next`, `volatile`, `is_async` and more. `ns-dep/command_metadata-…` returns one; nothing enumerates the set as records. | `liquers-core/src/command_metadata.rs:944`, `liquers-lib/src/commands.rs:286` |
| `MetadataRecord` is a closed struct — no application-defined attributes, so no tags. | `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` |
| `StoreCapabilities` has eight flags and a conformance suite keyed to them. | `liquers-core/src/store_conformance/mod.rs:97` |
| Query parameters can carry **any** string; `ActionRequest::encode` escapes. `-` is `~_`, space is `~.`, `/` is `~/`. | `specs/guides/QUERY_ESCAPING_GUIDE.md` |
| A dependency on **one directory's listing** is expressible (`Step::GetAssetDirectory` → `DependencyKey::from_dir_key`) but nothing ever registers a version for it, so the edge is silently dropped. | `liquers-core/src/plan.rs:2647`; filed as `DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED` |
| UI elements are query-driven: an element carries a query producing its content, and events are queries. A search field is therefore an input whose value is substituted into a search query. | `specs/reference/UI_INTERFACE_FSD.md` |
| `liquers-web` is **wasm32-only**. Tantivy, the mature Rust full-text engine, is server-oriented and has never committed to wasm. | `CLAUDE.md`; Tantivy wasm RFC |
| Six of ten assets API endpoints are 501 stubs, `listdir` among them. | `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` (P0) |
| No code in `liquers-core`, `liquers-lib` or `liquers-store` implements anything search-shaped. | grep, 2026-09-17 |

---

## 1. The unifying model

Every essential use case in `use-cases.md` is the same operation:

> **Select records from a set, by a predicate over their fields and their text, and return enough of
> each record to judge it and to address it.**

They differ only in **where records come from** and **which clause of the predicate is used**. That
gives three contracts and nothing else:

```
        record sources                predicate                     result
  ┌──────────────────────────┐   ┌──────────────────┐   ┌────────────────────────┐
  │ store subtree            │   │ text clause      │   │ identity  = a query    │
  │ asset listing            │──▶│ field clauses    │──▶│ fields to judge by     │
  │ command registry         │   │ (later: sql,     │   │ why it matched         │
  │ (later: rows, chunks)    │   │  similarity)     │   │ (later: score)         │
  └──────────────────────────┘   └──────────────────┘   └────────────────────────┘
        open set                    open set of clauses      one shape, forever
```

**Why this is the simplest common denominator**, rather than one abstraction among several:

- Full text and field filtering are *clauses of one predicate*, so the essential pair ships together
  (`use-cases.md` A1+A3).
- Command discovery is *a record source*, not a feature — nothing in the search path knows what a
  command is (`research-questions.md` §5).
- SQL is *an engine over the same records*, so a GlueSQL table's columns are the predicate's fields
  and neither needs its own view of the corpus (§4).
- RAG is *a record source plus a clause*: chunks are records, embeddings are derived assets, nearest-
  *k* is a clause. Hybrid lexical+semantic retrieval falls out of clauses composing (§8, §9).
- A third-party engine is *an implementation of selection*, so it can be added and removed without
  the consumers noticing (§3).

And the split between the two halves follows the axis each varies along
(`research-questions.md` §1): **projection is a command** (varies with the value type, open set),
**selection is a trait method** (varies with the backend, closed set, and the only place an index
can live).

### What the MVP of this actually is

The model is what keeps the design extensible. The *first version* is small:

- one record type and one predicate type in `liquers-core`;
- one selection method on `AsyncStore` with a default scan, plus a capability flag and conformance
  rules;
- one asset-level union that does not evaluate;
- one command in `liquers-lib` and one syntax front end for the user's search box.

Everything else in this document is an extension point, not first-version work.

---

## 2. Decision axes

| Axis | Question | Recommended |
|---|---|---|
| A | What is searched? | A3 — store entries, live assets and recipe-declared keys, without evaluating; commands as a further record source |
| B | Where does the capability live? | B2+B3+B5 — predicate and default scan in core, store hook, asset-level union, command surface in lib |
| C | How is a search expressed? | C2 — a serializable predicate; a small syntax front end compiles to it; C4 composition after it |
| D | What comes back? | D2+ — `AssetInfo` plus a per-hit match record, identity as a query, score reserved |
| E | How is it executed? | E1 as the contract; E3 (a store decorator) as the sanctioned index route; E4/E5 as overrides |
| F | What counts as a match? | F1 — text and field clauses; ordering promise stated from day one |
| G | How is it reached? | G1 — a query command; the UI search field and MCP both build queries; G2 deferred |
| H | How wide is one search? | H2 — rooted at a key, router fan-out specified |

### Axis A — What is searched

**A1. Store entries only.** Keys, stored metadata, stored bytes. What
`STORE-NO-CONTENT-OR-METADATA-SEARCH` literally asks for. Cannot see a derived asset that exists
only because a recipe declares it, which is half of what makes Liquers interesting.

**A2. Assets only.** Matches the position that the assets API is the client interface. Strictly more
work than A1 *and* dependent on it, since asset enumeration bottoms out in store enumeration.

**A3. The union, without evaluating.** Every store entry under the root; every live asset; every key
a recipe declares — described the way `get_asset_info` already resolves them. **Content matching
applies only where bytes already exist.** A recipe-declared key with no stored data matches on
fields or not at all.

Recommended: **A3**, with non-evaluation as a hard invariant (§4). A content search that reached
through to unevaluated recipes could recompute a corpus from one query.

Commands (`research-questions.md` §5) are a *fourth* source under the same union, added when it is
built rather than designed around later.

### Axis B — Where the capability lives

**B1. A command only.** Zero core change, days of work. Also exactly the failure on file: retrieval
logic in the caller, O(corpus) reads, no backend may do better, and a third copy next time.

**B2. A method on `AsyncStore` with a default implementation.** Every existing store keeps
compiling; a capable backend overrides. Needs a capability flag and conformance rules checking that
default and override agree.

**B3. An operation at the `AssetManager` level.** Required by A3 — only the asset layer knows about
live assets and recipes. Composes over B2.

**B4. A separate `SearchProvider` service on `Environment`.** Keeps the store trait narrow and makes
engines swappable by configuration. Costs a fourth service to construct, configure and document, and
re-raises "which store does it search" — which the router already answers for B2. Live alternative
if index lifecycle turns out to need its own home; Phase 2 should say why it is not needed rather
than ignore it.

**B5. A command surface in `liquers-lib`.** Not an alternative — every option needs it, because a
capability with no query surface is unreachable from HTTP, the UI, Python and wasm.

Recommended: **B2 + B3 + B5**, predicate and record types in `liquers-core` so all three speak one
vocabulary.

### Axis C — How a search is expressed

**C1. Fixed command arguments.** Trivial; every new dimension becomes a new positional argument.

**C2. A serializable predicate.** A structure a command can build, and an HTTP or MCP caller can send
as JSON. Because it is data, a backend can inspect it and decide what it can push down. This is what
makes B2's override worth having, and what lets a UI or an agent skip parsing entirely.

**C3. An expression mini-language.** The most expressive per character and the most expensive: a
second grammar, its own parser and errors, a much harder push-down story. A *small* front end
(`research-questions.md` §7) that compiles to C2 gets the ergonomics without the commitment.

**C4. Composition in the query language.** The search command returns a list; further commands
filter, sort and cut it. Liquers-native, costs nothing new, and cannot be pushed down — so it is the
right home for everything the predicate deliberately omits.

**C5. Regex.** The single hardest clause to push down anywhere. A later flag, not the interface.

Recommended: **C2 as the predicate, a §7-sized syntax as its front end, C4 for the rest.**

### Axis D — What comes back

**D1. Keys.** The caller immediately fetches metadata for every hit — the read amplification this is
meant to remove.

**D2. `Vec<AssetInfo>`.** Already a value variant, already serialized, already rendered, already
carrying the tiers that make a hit judgeable. Missing exactly two things: why it matched, and how
well.

**D2+. `AssetInfo` plus a per-hit match record** — matched field, short excerpt, reserved score — and
an identity that is a **query** rather than a key, because a command's address is not a store key
(§5) and a derived hit may have no key at all.

**D3. A new rich value type in `liquers-lib`.** Only if the result outgrows D2+; costs an `ExtValue`
variant, conversions and a `TypeInfo`.

**D4. A DataFrame.** Excellent for analysing a corpus; wrong as the primary result, since it puts an
optional feature on the critical path of a core capability. A conversion command from D2+.

Recommended: **D2+**.

### Axis E — How it is executed

**E1. Scan on every search.** Walk from the root, fetch metadata, fetch bytes only when a text clause
demands it, filter, stop at the limit. O(corpus) at the caller. Correct everywhere — **including
wasm** — and the default that lets every existing store claim the capability.

**E2. The index as a derived asset.** The most Liquers-native idea available, and better supported
than it first appears: a dependency on a *directory listing* already exists, so "a document was
added" is in principle visible to the dependency machinery. Three things must be established first:
the listing dependency is one level deep, not a prefix; nothing registers a version for it, so the
edge is currently dropped (`DIRECTORY-LISTING-DEPENDENCY-IS-NEVER-REGISTERED-OR-CHECKED`); and the
fan-out of a few hundred dependencies on one asset is unmeasured. A well-founded experiment, not a
first version.

**E3. A store decorator that maintains an index.** `IndexedStore<S>` wrapping any `AsyncStore`,
updating its index on the writes it performs and declaring the capability. This is the corrected form
of the Whoosh prototype's idea: the component that owns the invariant is the one that can enforce it,
and decoration is an established shape here. Honest limitation: it sees only writes made *through
it*, so a backend written by someone else falls back to scanning.

**E4. Push-down to the backend.** The point of B2's override — a service with server-side filtering,
a SQL-backed store, a store that already keeps a directory index. Free when available, absent
otherwise, which is why the default must exist.

**E5. An external engine.** Tantivy behind a feature, or a remote engine. Right at 100× scale, and
**not available on wasm**, so it can never be the baseline.

Recommended: **E1 as the specified contract, E3 as the sanctioned route to an index, E4/E5 as
overrides.**

### Axis F — What counts as a match

**F1. Text and field clauses.** Case-folded substring or term match on projected text; equality,
prefix and membership on named fields; glob or prefix on the key. Implementable identically in a
scan and in most backends, which is what makes push-down realistic.

**F2. Ranked full-text.** Tokenization, stemming, stop words, BM25. Changes the result contract —
order becomes meaningful — is language-dependent, and effectively forces E5.

**F3. Similarity.** §8/§9. A clause over a vector field, with the machinery living in commands and
derived assets rather than in search.

Recommended: **F1**, with the **ordering promise written from the first version**: results are
unordered unless a scoring clause was used; when one was, they are ordered by descending score and
each hit carries it. That single sentence is what lets F2 and F3 arrive without breaking a consumer
that learned to trust the order.

### Axis G — How it is reached

**G1. A query command.** Reachable through `/q`, the UI query console, `liquers-web` and
`liquers-py` with no per-surface work; cacheable as an asset; declarable in a recipe, so a saved
search is a first-class object. The UI search field is an input element whose value is substituted
into such a query — which is already how UI elements work.

**G2. A dedicated HTTP endpoint.** More conventional for non-Liquers clients; needs a specification
change to a document that six existing endpoints already contradict. Adding a tenth
specified-but-unimplemented endpoint to that surface is not the move.

**G3. An MCP tool.** Owned by `agent-memory-mvp`'s adapter; it builds a query rather than growing its
own engine (`research-questions.md` §6).

Recommended: **G1**, G3 mapping onto it, G2 deferred until the assets API is whole.

### Axis H — How wide one search is

**H1. Whole store.** Unbounded, and wrong the moment a store is large.

**H2. Rooted at a key.** The natural unit for the router too: a root at or below a mount dispatches
to one store; a root above one requires fan-out across the mounts beneath it, and merging. If the
router does not implement it, a search above a mount boundary silently sees one store — so it must
be specified either way, even if the first version refuses such a root.

**H3. Explicit multi-root.** Cheap once H2 exists; a later predicate field.

Recommended: **H2**.

---

## 3. Recommended combination

> **A3 · B2+B3+B5 · C2 with a small syntax front end · D2+ · E1 (E3 sanctioned, E4/E5 overriding) ·
> F1 with the ordering promise · G1 · H2**

A record type and a predicate type in `liquers-core`; an `AsyncStore` selection method with a default
scan, a capability flag and conformance rules; an asset-level union that never evaluates; a command
namespace in `liquers-lib` that builds the predicate from arguments or from a small search syntax and
returns hits carrying an address, the fields to judge by, and why they matched.

**Why not smaller.** B1 alone ships faster and has to be undone: it puts retrieval in the consumer
and forecloses push-down, which is the complaint already on file.

**Why not larger.** E2, E5, F2, F3 and the SQL adapter each add a dependency or a lifecycle no
essential use case needs, and each remains reachable because the record and the predicate were shaped
to admit it.

**What it unblocks:** `STORE-NO-CONTENT-OR-METADATA-SEARCH` closes; `agent-memory-mvp`'s open
question 3 is answered (search is a capability with a query surface, not a command private to
`ns-mem`); the user's search field and the agent's retrieval are the same mechanism with two front
ends.

---

## 4. Invariants

These hold regardless of which branch each axis takes, and belong in the reference document this
design produces.

1. **A search never evaluates.** It reads stored bytes, stored metadata, live asset metadata and
   recipe declarations. It never triggers computation, changes a status, or populates a cache.
2. **A search is bounded.** Every search has a root and a limit, and reports truthfully when it
   truncated. The `/q` handler's 30-second timeout is reachable.
3. **A search is addressable.** The result is a value produced by a query, so it composes, caches and
   can be declared in a recipe. A saved search is a recipe.
4. **Ordering is promised only with a score.** Unordered otherwise; deterministic for a fixed store
   state, but carrying no relevance meaning.
5. **The baseline runs everywhere, wasm included.** Any engine that does not is an optional override.
6. **No identity, no access control.** A search sees what the environment sees
   (`CORE-SESSION-AND-KEY-ACL`). Per-user visibility later is a filter over the same contract.
7. **A store that cannot select says so**, and the conformance suite checks that a declared override
   agrees with the default on the same corpus.

---

## 5. Deferred, and what keeps each cheap

| Deferred | Why not now | What keeps it cheap later |
|---|---|---|
| Ranked full text (F2) | Language-dependent; forces an engine that is not available on wasm | Score on the hit; ordering promise stated from day one |
| Semantic search and RAG (F3) | New runtime dependency, chunking and model choice | Chunks are a record source, embeddings are derived assets, nearest-*k* is a clause |
| SQL over records (GlueSQL) | An engine, not a predicate; optional dependency | Records carry *named typed fields*, so a column and a predicate field are the same thing |
| Index as a derived asset (E2) | Directory dependency is one level deep, currently dropped, and unmeasured at fan-out | Issue filed; nothing in the contract prevents it |
| Index in a store decorator (E3) | Not needed at current scale | It is an `AsyncStore` implementation — no consumer changes |
| External engine (E5) | Large dependency; unavailable on wasm | Lands as a selection override behind a feature |
| Tags | `MetadataRecord` is closed | Field lookup is by *name against a record*, not against struct fields |
| A `/search` endpoint (G2) | The assets API has six unimplemented endpoints already | The command is reachable through `/q` meanwhile |
| Router fan-out (S4) | Merge semantics and partial capability need specifying | Specified now, implemented later; a refused root is an honest first version |

---

## 6. Decisions Phase 2 must make

1. The predicate's exact clause set, and whether a backend may push down *part* of it — a partial
   push-down that silently drops a clause is a correctness bug, so the contract must be "push down
   and re-check" or "all or nothing".
2. Whether store-level and asset-level selection are one trait method or two, and how the union
   deduplicates a key present both as a live asset and in a store.
3. Router fan-out for a root above a mount boundary, including what happens when one mounted store
   cannot select — and whether the first version refuses such a root instead.
4. Whether a hit's identity is always a query, and how a store key renders as one.
5. How a record's text is obtained: directly from bytes for text media types, or through a projection
   command for values whose text is not their bytes — and how that decision is made without reading
   every byte in the corpus.
6. Whether the limit is a hard cut with a truncation flag or a cursor, and if a cursor, what it is
   stable against.
7. Whether search subsumes `listdir_asset_info` — an empty predicate at depth 1 is a listing — or
   stays separate.
8. Which of §5's two command-discovery routes to take: a record source over the registry, or a
   read-only virtual store that makes commands enumerable by `listdir` with nothing
   command-specific in the search path.
