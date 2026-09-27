# Indexation policy: what is searchable, and when it is produced

Companion to [Phase 1](./phase1-high-level-design.md). Two decisions that neither the record model
nor the interoperability layer answers:

1. **Not everything should be indexed.** A store holds sources *and* derived artifacts, caches and
   intermediates; indexing all of it pollutes every result.
2. **Some documents are indexed only when they are ready; others must be produced at indexation
   time if they are not available.** This is where volatile assets force a decision that can be
   avoided everywhere else.

Metadata is the easy case and is stated once here: **metadata is always indexable.** It exists for
every asset — from the store, from a live asset, or from a recipe for a key that has never been
evaluated — so a document is discoverable without anything being computed. Everything below is about
*content*.

---

## 1. The invariant that makes this coherent

> **Indexation may evaluate. Search may not.**

They are different acts with different budgets. A search is a user's question with a latency budget
and a scope the user chose; it must never start an asset
(`DESCRIBING-AN-ASSET-CAN-TRIGGER-ITS-EVALUATION`). Indexation is a scheduled job with a scope its
owner configured and a cost its owner accepted; producing a document is a legitimate thing for it to
do, when policy says so.

Stating it this way removes the apparent contradiction between "a search never evaluates" and "some
documents should be produced on indexation". Both are true, of different things.

---

## 2. Decision 1 — inclusion

Four layers, cheapest first. The first two need no metadata at all.

| Layer | Mechanism | Cost |
|---|---|---|
| **Scope** | the root, or the configuration query whose records are indexed | free — it is the thing already being configured |
| **Key patterns** | include/exclude globs, `.gitignore`-shaped | free; no metadata, no evaluation |
| **Type rules** | do not attempt text extraction for media types that have no text | free; the type is in metadata |
| **Per-asset declaration** | the asset says "not searchable" | needs an attribute mechanism (`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`); deferred |

In the general case inclusion needs no new mechanism at all: an external engine is configured with a
query, and **what that query yields is what is indexed**. Key patterns exist for the common case of
subtracting a subtree — build output, a cache directory — from an otherwise sensible root.

Two things worth stating plainly:

- **Exclusion is not a security mechanism.** This design has no identity and no access control
  (`CORE-SESSION-AND-KEY-ACL`). An excluded document is not protected, merely absent from one
  index — and possibly present in another.
- **A silently missing document is very hard to debug.** "Why is X not in my results?" needs an
  answer: excluded by pattern, not indexed yet, no content available, or genuinely not matching.
  Whatever the policy mechanism turns out to be, it should be able to explain itself for one key.

---

## 3. Decision 2 — content policy

Three values, per scope and ideally overridable per asset:

| Policy | Content indexed | Recipe evaluated | Use |
|---|---|---|---|
| **metadata-only** | none | never | binaries, huge files, anything whose text is meaningless |
| **when-ready** | whatever is already materialized | never | **the safe default** |
| **produce** | evaluated if absent | yes, at indexation time | documents whose content matters enough to pay for |

Under **when-ready**, a key that only a recipe declares is still *discoverable* — its description
comes from the recipe provider — but its content is not searchable until something evaluates it. That
is a good property rather than a limitation: **discoverability never requires evaluation**, and the
index tells the truth about what it has seen.

**produce** is the policy that needs care, because it converts indexation into computation of
unknown cost. A scope set to `produce` over a corpus of expensive recipes is a job, not a
refresh. Whatever form the configuration takes, the cost should be visible when it is set, not
discovered when it runs.

---

## 4. Four classes of document, and only one makes `produce` awkward

Content policy is decided per class, and the classes differ on one question: **is the document's
version knowable without producing it?**

| Class | Version available? | Content available without producing? | Reconciliation | `produce` |
|---|---|---|---|---|
| **Stored** — a source, or a derived asset that was persisted | yes, content hash | yes | version diff | not needed |
| **Transient** — non-volatile, deterministic, deliberately not stored | **yes — derivable from its dependencies' versions** | no | **version diff, computed without producing anything** | **needed, and unambiguously correct** |
| **Volatile** — use once, re-evaluated every time | **no** | no | schedule only | needed, but snapshot-only |
| **Ad-hoc query** — a non-keyed asset, a report generated on the fly | yes — derivable from the plan's dependencies, offline | no, and there is nowhere it could be | version diff, computed without producing | needed; and enumeration is the new problem (§5) |

### The transient class is the clean case for `produce`

A cheap report or dashboard rendered from precalculated data is not volatile: it stays valid exactly
as long as its inputs do. It is simply not worth storing, because reproducing it costs less than
reading it back.

Three properties follow, and together they make indexing such a document as sound as indexing a
stored one:

1. **Its version is a pure function of its dependencies' versions** — the same derivation
   `record-model.md` §3 uses for a chunk. So an indexer can decide whether its entry is stale
   **without producing it**, from metadata alone.
2. **What is produced stays valid** as long as the dependencies do, so the indexed content is not a
   snapshot of a moving target.
3. **Producing it is cheap by the author's own declaration** — that is the premise of the class.

Without `produce`, this entire class is permanently unsearchable by content, and the only workaround
is to store documents whose whole point is not being stored.

Two consequences worth naming. **The index becomes the only materialization** of such a document's
content, so rebuilding the index costs re-production. And in the interoperability layer the feed's
cost is asymmetric — the version scan stays cheap while `fetch` becomes a computation — which is
already the general shape (`fetch` is proportional to what changed) rather than a new problem.

**Liquers cannot currently express this class.** `CommandMetadata.cache` exists and is read by
nothing, `register_command!` cannot set it, and `PersistenceStatus::NotPersisted` means the write
*failed*. The available workaround — declaring the asset volatile — moves it into the weakest regime
and throws away the version that made it tractable. Filed as
`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT` (P2).

## 5. Ad-hoc queries: indexing something that is not a stored asset

A command may produce records from **queries** rather than from stored assets — a report generated on
the fly, parameterized and never written anywhere. The record's identity is then a query with no key
behind it.

**The record model already accommodates this, and this case is why.** Identity is the asset *as a
query* (`record-model.md` §1), not as a key; `AssetInfo` already carries `query: Option<Query>` and
`key: Option<Key>`, where `None` means "valid, but never written to a store"; and the asset manager
already holds query-identified assets in their own map (`assets.rs:4539`), with
`DependencyKey::from(&Query)` letting them participate in dependency tracking.

### What is genuinely new: there is no `listdir` for queries

A keyed corpus is **enumerable** — that is what `listdir` is. The space of queries is infinite, so an
index cannot discover which ad-hoc queries to hold. **The set must be declared.**

And the declaration already exists in this design: the **partition** of
[`record-model.md`](../record-streams/record-model.md) §3 is a list of `(chunk id, chunk query, version)`, and
nothing requires a chunk query to be key-rooted. A generator that yields arbitrary queries — one per
region, one per month — is the same mechanism with a different source. The construct built for
partial refresh turns out to be the enumeration mechanism for non-keyed assets.

### Versioning works, and offline

This is the part that makes ad-hoc queries tractable rather than merely possible. The planner derives
a query's dependency set **without evaluating it** — that is exactly what `liquers-validate` does,
parsing and planning with no store opened and no command run. So a version is available as a hash
over the plan's dependency versions, and staleness is decidable from metadata alone, exactly as for
the transient class.

One ingredient matters more here than anywhere else: **command implementation versions**. Liquers
already tracks them as dependencies (`ns-dep/command_metadata-…`, `ns-dep/command_impl-…`). For a
stored asset, a changed command is eventually noticed through changed content; an ad-hoc report has
no stored content to compare, so the command version is the *only* thing that says its output would
differ. It has to be in the version.

### Four things to be careful about

1. **Cardinality.** A report parameterized over a large dimension is a combinatorial explosion —
   one query per customer is a million index entries. The generator is responsible for bounding the
   set, and the policy should be able to cap it and say when it did.
2. **A hit is a query that must be re-run.** It is addressable, which is the property that matters,
   but re-running costs and may not reproduce what was indexed if dependencies moved. So the hit
   must carry **the version it was indexed at**, so a consumer can tell it is being shown what the
   query said at a point in time.
3. **The index is the only materialization**, more completely than for a transient keyed asset —
   there is nowhere else the content could live, so an index rebuild means re-running every query.
4. **Deduplication is not free.** Two different spellings may denote the same computation, and
   `Query::encode` canonicalizes form rather than meaning, so the same content can be indexed twice
   under two ids.

### What is missing at HEAD

The same gap as the transient class, one step further along: a non-keyed asset is never written to a
store, so it never receives `Version::from_bytes(content)` and its `metadata.version()` is `None`.
Query dependencies are therefore recorded with an unknown version today. The derived version
described above is not computed anywhere. This is the open question already recorded on
`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT` — what an unstored asset's version is computed over —
and the answer has to serve both classes.

## 6. Volatile assets force the choice

A volatile asset is "use once, then expires… never cached and must be re-evaluated each time"
(`metadata.rs:334`). Three consequences, and the third is a hard technical fact rather than a
judgement:

1. **It is never persistently ready.** So under `when-ready` its content is never indexed — not
   "indexed late", but never. If it is to be in an index at all, `produce` is the only policy that
   puts it there.
2. **Any snapshot is stale immediately.** What `produce` stores is what the document said once. For
   a volatile document that is a fact about the past, and a consumer should be able to tell that is
   what it is getting.
3. **Volatile assets never register a version.** Version registration is gated on
   `Status::Ready | Source | Override` (`assets.rs:5583`, `:5715`), and `Volatile` is not among
   them. So **the `(id, version)` reconciliation diff of `interoperability-layer.md` §3 has nothing
   to compare for a volatile entry.** Treated as "no version", such an entry either never looks
   changed or always looks changed, depending on how absence is read — and neither is right.

So volatile entries need **time-based refresh rather than version-based**: the index records *when*
it produced the entry, and a schedule decides when to produce it again. `Expires` is the existing
vocabulary for exactly that, which is the same mechanism the interoperability layer already uses to
declare how stale a view may be.

**Volatility is knowable before evaluation**, which is what makes the policy applicable at all:
`plan.is_volatile` is computed at plan time (`recipes.rs:281`), and `AssetInfo.is_volatile` and
`MetadataRecord.is_volatile` report it. An indexer can therefore apply a volatile-specific policy
without running anything.

---

## 7. Refresh: two regimes, not one

| Asset kind | Change detection | Refresh trigger |
|---|---|---|
| Stored source | content-hash `Version` | version diff |
| Derived, non-volatile | content-hash `Version` | version diff, plus the expiration cascade as the fast path |
| Recipe-declared, not produced | the recipe's version | version diff on the recipe |
| **Transient** — deterministic, not stored | version derived from dependencies | version diff, decided without producing |
| **Ad-hoc query** — non-keyed | version derived from the plan's dependencies and command versions | version diff, decided without producing |
| **Volatile** | **none available** | **schedule (`Expires`)** |

The interoperability layer assumed one regime. It needs both, and the second must be visible in the
entry: an index cannot report a volatile entry's freshness as though a version vouched for it.

---

## 8. Where the policy lives

- **In the index or sink configuration**, per scope, pattern-based. Needs nothing new, and it is
  where the cost is being accepted. **Recommended first.**
- **On the asset**, as a declaration. More precise and more local, and it needs an attribute
  mechanism that does not exist yet.
- **On the recipe**, for derived assets. Arguably the most natural home for a `produce` decision,
  since the recipe is where the cost is described.

Precedence between them is an open question and should be settled before two of them exist.

---

## 9. What the MVP does

**Nothing configurable — and that is not a gap.** M1 and M2 are a scan with no index, so:

- **Inclusion** is the root plus the predicate the user wrote.
- **Content policy** falls out of the search invariant rather than from configuration: a search may
  not evaluate, so it reads content that exists and treats everything else as metadata-only. That is
  `when-ready`, arrived at for free.
- **Volatile** content is never read, for the same reason, and the entry is still discoverable by its
  metadata.
- **Type rules** are the one real decision: do not attempt text extraction on a media type with no
  text. Cheap, and needed the moment a store holds an image.

Policy configuration becomes necessary at **M4** (a built-in index) and **M7** (an external sink) —
the milestones where something is written down ahead of a query and can therefore be wrong.

---

## 10. Open decisions for Phase 2

1. Whether `produce` exists in the first indexing version. An earlier draft leaned towards leaving
   it out as the smaller option; the transient class (§4) argues the other way, because without it a
   whole category of cheap derived views is permanently unsearchable by content and the only
   workaround is to store them. If it is left out, say so as a known limitation rather than as an
   oversight.
2. What an index entry records for a volatile document — a production time, and whether it is marked
   so a consumer knows the freshness is time-based rather than version-vouched.
3. Policy granularity and precedence: scope configuration, per-asset declaration, per-recipe
   declaration.
4. Whether exclusion hides a key entirely or only its content, and whether an excluded key is
   distinguishable from an absent one.
5. How "why is this document not in my results?" is answered for one key — a diagnostic the design
   should be able to provide, given how hard silent absence is to debug.
6. Whether a `produce` scope's cost is estimable before it runs, or whether the honest answer is
   that it is a job the operator schedules and watches.
7. How a declared query set is bounded and what happens when it exceeds the bound — refuse, cap and
   report, or index lazily.
8. Whether two query spellings denoting the same computation are deduplicated, and if so on what.
