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
visible in the diff (T2 ✔ in practice), arbitrary fields (T7 ✔). The search branch already parses
it (`ns-search/field_extract`).

Against: a listing must open every file (T1 ✗ — `field_extract` is a scan, cached as an asset but
recomputed when any file changes); Markdown only (T6 ✗); and nothing a Liquers *client* sees —
`AssetInfo`, the assets API, a UI — knows about it.

### C. A front-matter store: project front-matter into metadata

An `AsyncStore` adapter wrapping any backend. For an entry whose media type is Markdown it parses
the front-matter on read and **projects** `title` and `description` into the `MetadataRecord` it
returns from `get_metadata` / `get_asset_info`; everything else is passed through. The file stays
the single source of truth; the metadata is a view of it.

```
FrontMatterStore<S: AsyncStore>   // liquers-lib (needs a YAML parser); wraps the corpus mount
  get_metadata(key)  = inner.get_metadata(key) ⊕ project(front_matter(inner.get(key)))
  get_asset_info(key)= same projection
  set(key, data, md) = inner.set(key, data, md)   // MVP: front-matter in data wins on next read
```

For: T1 ✔ through every existing interface (assets API `listdir`, `AssetInfo`, the UI, `catalog`),
T2 ✔ by construction (the projection is recomputed from the bytes, cached by the inner version),
T3 ✔ (the file is the truth), T6 ✔ (non-Markdown passes through with ordinary metadata — option A
for everything that is not Markdown). This bridges A and B: *Markdown documents keep their tiers in
the text, and every client still reads them as metadata*.

Against, and what has to be decided:
- **A store that parses content** crosses a line every existing store keeps: none of them
  interprets the bytes it holds (`STORE_SEMANTICS.md` specifies storage and metadata, not content;
  the nearest exception is `get` repairing an unparseable *sidecar*). The adapter is therefore not a backend but an explicit, opt-in decorator, like a
  router mount — and it lives in `liquers-lib`, not `liquers-core`.
- **Cost on listing:** projecting a folder's metadata reads every Markdown file once; after that a
  cache keyed by `(key, inner version)` makes it a metadata read. On a backend that synthesizes
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
heading as title, first paragraph as description; the search design's P3 rule) or with a language
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
consistency. Rejected; its one strength — a per-folder overview — is covered by T5 below.

## 3. Recommendation

| Layer | Holds | Option |
|---|---|---|
| Source of truth for Markdown memories | L0 `title`, L1 `description`, other fields, L2 body | **B** — front-matter in the file |
| What clients read | `AssetInfo.title` / `.description` for every entry | **C** — the front-matter store projects B into metadata; non-Markdown uses ordinary metadata (A) |
| Entries without front-matter, folders | Fallback and generated tiers | **D** — deterministic rule in the MVP; a language-model derivation is a later, optional command |
| Filtering on other fields | `status`, `kind`, `area`, dates | `ns-search/field_extract` records until `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` lands |

**Folder tiers (T5):** a folder's L0/L1 come from the front-matter of its `README.md` (or
`_index.md`) when present — the convention `specs/README.md` already follows and the LLM-wiki
pattern uses — projected onto the folder's `AssetInfo` by the same adapter (the default `AsyncStore::get_asset_info` builds a directory's info from `default_metadata` without reading anything, so the adapter overrides the directory case); otherwise from a derived
overview (D).

**Tier lengths** are conventions checked by `ns-mem/check`, not types: L0 one line (≤ 120
characters), L1 ≤ ~500 characters. OpenViking's budgets (256 and 4,000 characters) are for
generated text covering a whole folder; hand-written per-document front-matter is shorter, and
`specs/` titles already meet the L0 rule.

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
