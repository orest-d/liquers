---
id: RECORD-STREAMS-CHUNKING
kind: analysis
title: Chunk generation, unknown chunk counts, and store-backed resumability
workflow: liquers-project
status: draft
area: [lib/value, core/assets]
created: 2026-09-20
---
# Chunk generation, unknown chunk counts, and store-backed resumability

Written against a review observation: a SQL source *can* use `SourceBacking::Manifest` if the offset
is a command argument — but the manifest as designed **requires knowing how many chunks there are**,
which SQL normally does not, and `COUNT(*)` can be expensive. Plus two further ideas: generating
chunk queries from a **template**, and converting a manifest **to recipes** so chunks are stored and
a long run can resume after a restart.

**Verdict up front.** Most of this is not in scope, but **three changes are cheap now and expensive
later**, and one of them is decisive because the manifest is a *persisted* format. They are in §5.

## 1. Offset as an argument does work, and the query mechanics are clean

A chunk query is the base query with offset and limit appended to its **last action**, and the chunk
number inserted into the filename. Both are structural rather than textual, because
`TransformQuerySegment` keeps them as separate fields (`query.rs:1114-1121`):

```rust
pub struct TransformQuerySegment {
    pub header: Option<SegmentHeader>,
    pub query: Vec<ActionRequest>,        // append to .last_mut()
    pub filename: Option<ResourceName>,   // insert before the extension
}
```

So the template renders through `ActionRequest`, never by string templating — the rule already stated
for `LocatorRule`. Validated with `liquers-validate`:

```
-R-key/db/conn.yaml/-/ns-sql/table-mytable/data.csv            base
-R-key/db/conn.yaml/-/ns-sql/table-mytable-0-100/data_0000.csv chunk 0
-R-key/db/conn.yaml/-/ns-sql/table-mytable-1000-100/data_0010.csv chunk 10
```

All three parse and plan. The filename caveat matters for a concrete reason beyond tidiness: in
Liquers the filename carries the output format, and when chunks are **stored** it also determines
their keys. `data.csv` → `data_0010.csv` → `dir/data_0010.csv` is one rule serving both.

## 2. Manifest and template are not alternatives — they are two fields of one thing

The natural reading is a new enum variant beside `Manifest`. That is wrong, and seeing why is the
most useful structural finding here.

A store-backed run **is a template whose already-computed prefix has been memoized**. During
execution the known list grows while the template stays the same. They coexist:

| State | known chunks | template |
|---|---|---|
| A fixed list of files | populated | none |
| A fresh SQL source | empty | present |
| A SQL source half-way through a long run | partial | present |
| Chunks all computed and stored | populated | present, now unused |

So:

```rust
enum SourceBacking {
    /// Batches already in memory.
    Materialized(Vec<Arc<RecordBatch>>),
    /// Chunks named by queries. `known` is the manifest — which may be the
    /// memoized prefix of `template` rather than the whole list.
    Queried {
        known: Vec<ChunkEntry>,
        /// `None` — `known` is complete. `Some` — more chunks may exist beyond it.
        template: Option<ChunkTemplate>,
        /// Where computed chunks are persisted, if they are.
        keys: Option<ChunkKeys>,
    },
}

struct ChunkEntry { query: Query, key: Option<Key>, version: Option<Version> }
```

`Manifest(Vec<Query>)` is the degenerate case: `known` populated, `template` and `keys` absent.

## 3. The unknown-count problem, and what it costs

### Termination

An unbounded source ends when a chunk comes back **short** — fewer rows than the batch size. A chunk
returning *exactly* the batch size is ambiguous, so the final probe is always one extra round trip
that returns nothing. Inherent to pagination; worth stating so it is not read as a bug.

### `chunks()` cannot return a complete `Vec` — the one API that must change now

The current signature is `fn chunks(&self) -> Vec<ChunkDescriptor>`: synchronous, complete, and it
**bakes in a known count**. Every consumer written against it assumes enumeration is possible.

```rust
pub enum ChunkList {
    /// Every chunk known. Reconciliation can diff a complete set.
    Known(Vec<ChunkDescriptor>),
    /// Count unknown. Walk from `first` by `stride` until a short chunk.
    Unbounded { first: ChunkDescriptor, stride: ChunkStride },
}
```

Even with only `Known` ever constructed today, having the enum means reconciliation and the
interoperability layer are **written to handle both from the start**. Retrofitting it means revisiting
every consumer.

### Reconciliation degrades, and this is a real limitation

The interoperability layer's contract is a set-diff over `(ChunkId, Version)` pairs, which detects
additions, changes **and deletions**. That requires enumeration.

| Source | Additions | Changes | Deletions |
|---|---|---|---|
| `Known` | yes | yes | **yes** |
| `Unbounded` | yes, at the end | only by walking | **no** |

So an unbounded source supports **append-only reconciliation** cheaply, and full reconciliation only
at the cost of a complete walk — which is the same cost as re-feeding. For logs and event tables that
is fine; for a mutable table it is a genuine gap. It belongs in the reference, not in a reader's
surprise.

### Offset pagination needs a total order, and the `Id` invariant supplies it

`OFFSET n LIMIT m` without a **total** order gives non-deterministic pages: ties break arbitrarily,
so a row can appear twice or not at all across chunks. The schema already requires exactly one unique
`Id` field, so `ORDER BY <id>` is available by construction — the invariant that exists for
reconciliation pays a third time (after keyset chunking, after delete-by-term).

Even so, offset pagination over a **changing** table is inconsistent between chunks. A snapshot
(repeatable-read) fixes it and holds a transaction open for the whole run, which is usually
unacceptable for a long one. Store-backed chunks (§4) are the better answer: each chunk is consistent
in itself and its capture time is recorded.

### Keyset is faster but sequential — and store caching dissolves the conflict

| Stride | Cost per chunk | Random access |
|---|---|---|
| `Offset { batch_size }` | O(offset) on many engines; Spark especially | **yes** |
| `Keyset { batch_size }` | O(log n) with an index on the id | **no** — chunk *n* needs the last id of *n−1* |

Keyset's sequential dependence looks fatal for resuming at chunk 7 — but with §4, chunks 0–6 come
from the **store**, not the database, and only the new tail is computed, which is sequential anyway.
**The two ideas fit together**, and neither is as good alone.

### Expensive offsets (Spark)

No general fix, and the review is right that store caching is the answer. Two narrower ones worth
recording: use keyset where an index exists (not Spark), and — for Spark specifically — have the
engine **write partitioned output** and point the manifest at those files, pushing partitioning to
the source rather than paginating it.

## 4. Store-backed chunks: the manifest is already a recipe

This is the strongest idea in the review, and it needs almost nothing new — because what it describes
**is what a Liquers recipe already is**: a key, produced by a query.

Give each `ChunkEntry` a key, and evaluation becomes: *if the key is in the store, load it; otherwise
evaluate the query, store the result, and record it*. That is recipe semantics, so the existing asset
machinery supplies store-backed caching, dependency tracking and expiration without record-streams
knowing about any of them.

**Resumption after restart falls out and needs no restart logic at all.** A computed chunk is a file
that exists; the manifest is the record of which exist. On restart, load the manifest, fetch what is
there, continue the template from its length. The prototype already did the durable half —
`_store_batches` rewrites its manifest after *every* batch, so a run that dies leaves its finished
chunks usable.

The "keys schema" the review asks for is also already solved there:
`StoredDataframeIterator { key, item_keys, extension, number_format }` — a directory, a number format
(`"%04d"`) and an extension.

```rust
struct ChunkKeys {
    /// Directory the chunks live under.
    dir: Key,
    /// Chunk-number formatting, e.g. "{:04}".
    number_format: String,
}
```

No `extension` field: the filename rule of §1 already produces `data_0010.csv`, so the extension comes
from the base query and one rule serves both naming and keys.

**Three things this raises that are not free:**

1. **Invalidation.** A stored chunk is a cached answer to a query against an external database, which
   can change underneath it. Recipes handle this through dependency versions, but a SQL source has no
   Liquers dependency to version — so staleness is time-based or manual. This is the same gap as
   `ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT` and the expiration work, not a new one.
2. **Cleanup.** Already an open question for manifests, and keys sharpen it: a manifest that owns a
   directory of stored chunks should be able to drop them, which the query-based manifest cannot do
   on its own. The prototype could, because it held store *keys* and cleaned its directory before
   rewriting.
3. **Concurrency.** Two runs sharing a manifest directory can both compute the same chunk. The asset
   layer's job-queue deduplication covers the in-process case; two processes need the store's write
   semantics, which `STORE-WRITE-HAS-NO-PRECONDITION` says are not there yet.

## 5. What to change now, and what not to

**Adopt now — cheap, and expensive later:**

| # | Change | Why it cannot wait |
|---|---|---|
| 1 | `chunks()` returns `ChunkList { Known, Unbounded }` rather than `Vec<ChunkDescriptor>` | Consumers written against a complete `Vec` all assume enumeration. Retrofitting touches reconciliation and the interoperability layer |
| 2 | `SourceBacking::Queried { known, template, keys }` instead of `Manifest(Vec<Query>)`, with `template` and `keys` always `None` in the first version | **The manifest is a persisted format.** Changing its JSON shape later breaks every stored manifest. `{known:[…], template:null, keys:null}` stays readable |
| 3 | `SourceBacking` stays private; behaviour goes through `RecordSource` methods | `CLAUDE.md` forbids default match arms, so a new variant is a compile error at every match. Keeping matches to one module makes that a localized change rather than a sweep |

**Do not build now:** the template renderer, keyset stride, store-backed chunk evaluation,
manifest-to-recipe conversion, or resumption. All are additive once 1–3 are in place, and all belong
with `NO-RELATIONAL-DATABASE-ACCESS-LAYER` where the motivating case lives.

**Record in the reference when it is written:** that an unbounded source cannot detect deletions;
that offset pagination requires a total order and the `Id` field provides it; and that the last probe
of an unbounded walk always returns nothing.
