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

## 4. Volatile assets force the choice

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

## 5. Refresh: two regimes, not one

| Asset kind | Change detection | Refresh trigger |
|---|---|---|
| Stored source | content-hash `Version` | version diff |
| Derived, non-volatile | content-hash `Version` | version diff, plus the expiration cascade as the fast path |
| Recipe-declared, not produced | the recipe's version | version diff on the recipe |
| **Volatile** | **none available** | **schedule (`Expires`)** |

The interoperability layer assumed one regime. It needs both, and the second must be visible in the
entry: an index cannot report a volatile entry's freshness as though a version vouched for it.

---

## 6. Where the policy lives

- **In the index or sink configuration**, per scope, pattern-based. Needs nothing new, and it is
  where the cost is being accepted. **Recommended first.**
- **On the asset**, as a declaration. More precise and more local, and it needs an attribute
  mechanism that does not exist yet.
- **On the recipe**, for derived assets. Arguably the most natural home for a `produce` decision,
  since the recipe is where the cost is described.

Precedence between them is an open question and should be settled before two of them exist.

---

## 7. What the MVP does

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

## 8. Open decisions for Phase 2

1. Whether `produce` exists in the first indexing version at all, or whether an index only ever
   reflects what something else materialized. The latter is smaller and forecloses nothing.
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
