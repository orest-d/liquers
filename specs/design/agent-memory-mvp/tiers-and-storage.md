# Tiers and storage — where L0, L1 and L2 live

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-11. It answers one question
in depth: OpenViking serves context at three levels — an abstract (L0, ≤ 256 characters), an
overview (L1, ≤ 4,000 characters) and the original content (L2) — and Liquers already has `title`, `description` and the value. Is
the metadata the right home for the tiers, is Markdown front-matter, or is there a bridge?

## 1. What the tiers are for

Tiering is a context-budget mechanism, not a data model. An agent browsing a folder of 300 issues
must decide which three to read; it can afford ~10–20 tokens per entry for that decision, ~100–300
for the shortlist, and the full text only for what it reads. Every system in
[`memory-systems-survey.md`](./memory-systems-survey.md) that scales past a few hundred memories has
some form of it — OpenViking explicitly (L0/L1/L2, as `.abstract.md` / `.overview.md` sidecars per
directory, generated bottom-up by a language model), Claude Code's auto memory (`MEMORY.md`, one line
per memory, then topic files), Claude skills and this repository's `SKILL.md`
files implicitly (`description` loaded, body on demand), the LLM-wiki pattern through `index.md`
(one line per page) — and the ones that do not (flat vector memories) compensate by returning
short extracted facts instead of documents.

So the requirements are:

| # | Requirement | Why |
|---|---|---|
| T1 | One listing returns L0 and L1 of every entry, **without reading L2** | The point of tiering. A listing that opens every file is a scan |
| T2 | L0/L1 cannot silently go stale when L2 changes | A wrong abstract is worse than none: it steers the agent away |
| T3 | A human (and git) can read and edit all three | Memory that only a service can see cannot be reviewed — the whole `specs/` process depends on review |
| T4 | Entries that arrive without L0/L1 still get them | Notes, imported documents, plain Markdown |
| T5 | Folders have tiers too | Browsing needs "what is in this folder" without listing it. OpenViking keeps its tiers *only* per folder; it has since dropped recursive directory descent from retrieval (now one scoped vector search), but still uses folder L0/L1 for navigation and reranking |
| T6 | Works for non-Markdown values | A memory corpus also holds tables, JSON, images, query results |
| T7 | Other fields (status, kind, area, dates) are reachable the same way | Filtering needs them; §1.11 of the search design measured lifecycle state as the one filter that needs metadata |

## 2. Five options

### A. Metadata-native: `MetadataRecord.title` / `.description`

The tiers already exist (`liquers-core/src/metadata.rs` `MetadataRecord::title`, `description`;
`AssetInfo` likewise), recipes and `Context::set_title` / `set_description` populate them, and the
assets API's `listdir` returns `AssetInfo` for a folder in one call (T1 ✔, T6 ✔).

Against: in `AsyncFileStore` they live in a `.__metadata__` JSON sidecar — invisible in an editor,
noisy in git, and lost by any tool that copies the Markdown without the sidecar (T3 ✗). Nothing
updates them when the file is edited outside Liquers (T2 ✗). There is no room for other fields
(`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`; T7 ✗).

### B. Front-matter-native: YAML at the top of the Markdown

What `specs/` does, what Claude skills, Basic Memory, Obsidian, Jekyll/Hugo and the LLM-wiki
pattern do. `title` and `description` (or `summary`) are front-matter fields; anything else is too.

For: human- and git-native (T3 ✔), the abstract changes in the same edit as the text, so drift is
visible in the diff (T2 ✔ in practice), arbitrary fields (T7 ✔). `ns-search/field_extract` already parses
it.

Against: a listing must open every file (T1 ✗ — `field_extract` is a scan, cached as an asset but
recomputed when any file changes); Markdown only (T6 ✗); and nothing a Liquers *client* sees —
`AssetInfo`, the assets API, a UI — knows about it.

### C. A front-matter store: project front-matter into metadata

An `AsyncStore` adapter wrapping any backend. For an entry whose media type is Markdown it parses
the front-matter on read and **projects** `title` and `description` into the `MetadataRecord` it
returns from `get` and `get_metadata` (and into `get_asset_info`); everything else is passed
through. The file stays
the single source of truth; the metadata is a view of it.

```
// Illustrative only — Phase 2 owns the shape.
FrontMatterStore<S: AsyncStore>
  get(key)           = (data, inner metadata ⊕ project(front_matter(data)))
  get_metadata(key)  = inner.get_metadata(key) ⊕ project(front_matter(inner.get_bytes(key)))
  get_asset_info(key)= the same projection; for a directory, from its README.md
  set_metadata(key,m)= rewrite the front-matter from m's title/description, then inner.set
  everything else    = forwarded to inner
```

Two placement facts constrain where it sits (codebase review, 2026-10-11):

- **`get` must project, not only `get_metadata`.** `AssetManager::get_asset_info` answers from the
  live asset whenever one is loaded (`liquers-core/src/assets.rs` around line 6141), and that
  asset's metadata came from `store.get`. A projection only in `get_metadata` would vanish as soon
  as the document is read once. A loaded asset still holds the projection of the bytes it loaded;
  how it learns of an outside edit is the question below.
- **It must wrap the router, or the router must forward.** `AsyncStoreRouter` forwards `get` and
  `get_metadata` to its members but not `get_asset_info` or `listdir_asset_info`; those run the
  trait defaults on the router itself, whose directory case reads nothing
  (`STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO`, filed 2026-10-11). So folder tiers work only with the
  adapter wrapping the whole router (`FrontMatterStore<AsyncStoreRouter>`), or after that issue is
  fixed. File tiers work either way, because they ride `get_metadata`.
- **It must forward the whole trait.** Most `AsyncStore` defaults are wrong for a wrapper
  (`key_prefix` empty, `is_supported` false, `listdir` empty, `set`/`remove`/`makedir` refusing), and
  the router calls `default_metadata` and `finalize_metadata*` on its members.

For: T1 ✔ through every existing interface (assets API `listdir`, `AssetInfo`, the UI, `catalog`),
T2 ✔ provided the cache notices outside edits (below),
T3 ✔ (the file is the truth), T6 ✔ (non-Markdown passes through with ordinary metadata — option A
for everything that is not Markdown). This bridges A and B: *Markdown documents keep their tiers in
the text, and every client still reads them as metadata*.

Against, and what has to be decided:
- **A store that parses content** crosses a line every existing store keeps: none of them
  interprets the bytes it holds (`STORE_SEMANTICS.md` specifies storage and metadata, not content;
  the nearest exception is `get` repairing an unparseable *sidecar*). The adapter is therefore not
  a backend but an explicit, opt-in decorator. It lives in `liquers-lib` with the rest of `ns-mem`
  because it is a memory policy, not a storage primitive (a YAML parser is available in both crates).
- **Freshness after outside edits.** A cache keyed by `(key, inner version)` is unsound: the version
  in a sidecar does not change when a file is edited by another program, and it is re-hashed only
  when bytes are read (`VersionVerification::OnRead`; the precedent is
  `STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS`). The projection cache must key on cheap
  file facts (size and modification time) or re-hash, and a loaded asset whose bytes changed must be
  refreshed on the next `key/info` (AC-11).
- **Cost on listing:** projecting a folder's metadata reads every Markdown file once; after that
  the cache above makes it a metadata read. On a backend that synthesizes
  versions lazily the first listing is a scan. Acceptable at the scale of `specs/`; measured in
  Phase 3.
- **Write-back:** what happens when a client sets `title` through metadata (`POST key/description`
  on the assets API) on a Markdown entry? Either the adapter rewrites the front-matter (both views
  stay equal, the content hash changes) or it refuses. Recommended: rewrite, because refusing makes
  the assets API lie about what is settable.
- **Fields beyond the two tiers** (T7) still have no place in `MetadataRecord`; until
  `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` is resolved they are reached through
  `field_extract` records, not through metadata. C does not block on that issue.
- **`AsyncFileStore` persists a sidecar on first read** (`STORE-NO-READ-ONLY-ADAPTER`). The adapter
  must not write the projected metadata back through the inner store, or the sidecar becomes a
  stale second copy.

### D. Derived tiers: L0/L1 computed by a command, cached as assets

For entries with no front-matter (T4), a recipe derives the tiers — deterministically (first `# `
heading as title, first paragraph as description — proposal P3 of `store-and-asset-search`'s
`agent-queries.md`; or a section the collection names, such as an issue's `## Problem`) or with a language
model (OpenViking's approach). As recipes they are assets: versioned, cached, and recomputed when
the source's content hash changes (T2 ✔). Folder tiers (T5) are a derivation too: an overview of
a folder is a recipe over its children's L0s.

For: the only option that covers T4 and T5 automatically, and the Liquers-native one — it is what
the asset layer was built for. Against: a language-model derivation adds a provider dependency,
cost and non-determinism; and a derived tier is not editable — a human correcting it must turn it
into front-matter (B), which is the right promotion path anyway.

### E. Tier files beside the content (OpenViking's layout)

Separate `.abstract.md` / `.overview.md` files per folder (OpenViking) or per document. For: no parsing, cheap
listing. Against: three files per memory, which drift independently (T2 ✗) and triple what a human
reviews; Liquers already has a sidecar for exactly this purpose (A), so E is A with worse
consistency. Rejected; its one strength — a per-folder overview — is requirement T5 (§1), met in §3.

## 3. Recommendation

| Layer | Holds | Option |
|---|---|---|
| Source of truth for Markdown memories | L0 `title`, L1 `description`, other fields, L2 body | **B** — front-matter in the file |
| What clients read | `AssetInfo.title` / `.description` for every entry | **C** — the front-matter store projects B into metadata; non-Markdown uses ordinary metadata (A) |
| Entries without front-matter, folders | Fallback and generated tiers | **D** — deterministic rule in the MVP; a language-model derivation is a later, optional command |
| Filtering on other fields | `status`, `kind`, `area`, dates | `ns-search/field_extract` records until `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` lands |

**Folder tiers (T5):** a folder's L0/L1 come from the front-matter of its `README.md` (or
`_index.md`) when present — the LLM-wiki pattern's convention; `specs/README.md` has no front-matter
today and would gain it — projected onto the folder's `AssetInfo` by the
same adapter wrapping the router (§2 C); otherwise from a derived
overview (D).

**Tier lengths** are conventions checked by `ns-mem/check`, not types: L0 one line (≤ 120
characters), L1 ≤ ~500 characters. OpenViking's budgets (256 and 4,000 characters) are for
generated text covering a whole folder; hand-written per-document front-matter is shorter.

**The existing corpus has no L1.** At HEAD, no document under `specs/` has a `description:` field
(the issue template written by `docs_index.py new` has none), and 6 issue titles exceed 120
characters. So for `specs/` the L1 comes from a **per-collection fallback** declared in the schema —
the first paragraph of an issue's `## Problem`, of a design's Phase 1 `## Purpose`, of a reference's
first section — until descriptions are backfilled (Phase 1 open question 3). The over-long titles are
`check` warnings, not errors.

**Escalation path:** C is the only part that is new code in the storage path. If Phase 2 finds it
crosses that storage/interpretation line too far, the fallback is to keep B as truth and serve tiers only
through `field_extract` records (no metadata projection). That loses T1 through the generic assets
API but keeps the MCP tools working, because they can call `field_extract` themselves.

## 4. Consequences for the rest of the design

- The MCP `memory_list` / `memory_search` results carry `title` and `description` straight from
  `AssetInfo` (C) — no tool-specific parsing.
- `memory_write` of a Markdown memory takes `title` and `description` as arguments and writes them
  as front-matter, so an agent cannot create a memory without L0/L1 (T4 at the source).
- `ns-mem/check` reports Markdown memories with no `title` or `description`, or an L0 over length.
- A language-model summarizer, when it comes, is a command producing *proposed* front-matter; an
  agent or human accepts it into the file. It never writes tiers that look hand-written.
