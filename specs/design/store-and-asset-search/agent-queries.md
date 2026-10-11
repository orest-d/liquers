# Agent queries — what an agent asks, and how it is expressed

> **Accepted 2026-10-11.** Proposals P1–P3 and both simplifications are now in Phase 1 (AC-17,
> AC-28 to AC-30) and Phase 2. `field_extract` also reads a folder directly, so the forms below no
> longer need a `catalog` in front of it, except where content is also wanted (L1).

Companion to Phase 1 revision 8, written 2026-10-11. It tests the design against the questions an
agent actually asks, from three angles:
1. this repository's `specs/`;
2. a general agent-memory system in the style of OpenViking;
3. the tools a coding agent (Claude Code) uses when navigating this repository.

Every encoded query below was checked with `liquers-validate`, which also confirmed that it decodes
to the expression shown. The decoded form is what a search box or an MCP tool would accept. The
encoded form is what goes into a URL.

Escapes used in the encoded forms:

| Escape | Character |
|---|---|
| `~.` | space |
| `~ncolon~` | `:` |
| `~nast~` | `*` |
| `~nquot~` | `"` |
| `~nlpar~`, `~nrpar~` | `(`, `)` |
| `~nverbar~` | `\|` |
| `~ngt~` | `>` |
| `~/` | `/` |
| `~_` | `-` (`_` needs no escape) |
| `~nbsol~`, `~nplus~` | `\`, `+` |

## 1. This repository's `specs/`

| # | Question | Decoded query | Works today (in design)? |
|---|---|---|---|
| L1 | Open issues about expiration | `-R-key/specs/issues/-/ns-search/catalog-t-t/field_extract/search-<expr>` with `expiration -status:closed*` | yes |
| L2 | Designs in the architecture phase | `…/specs/design/-/ns-search/field_extract/search-<expr>-f-0` with `filename:DESIGN.md phase:architecture` | yes |
| L3 | P0/P1 issues in `core/store` | `area:core/store (priority:P0 \| priority:P1)`, after `field_extract`, with `rank=false` | yes; `area` is a list, which matches any element |
| L4 | Is there already an issue for this? (duplicate check before filing) | `search-store metadata search-t-f-5-title`, after `field_extract` | yes; BM25 over titles |
| L5 | Which documents mention `STORE-NO-CONTENT-OR-METADATA-SEARCH`? | `-R-key/specs/-/ns-search/catalog-t-t/search-<id>-f-0` | yes |
| L6 | What was filed recently? | `created:>2026-10-01`, after `field_extract` | yes, with ISO dates compared as text. Ordering by date needs `sort` ([`record-filter-commands`](../record-filter-commands/)) |
| L7 | Issues belonging to a design | `design:store-and-asset-search`, after `field_extract` | yes |
| L8 | Which reference explains predecessor boundaries? | `-R-key/specs/reference/-/ns-search/catalog-t-t/search-"predecessor boundary"` | yes, at file level |
| L9 | *The section* that explains it | — | **no**: results are whole files. See proposal P2 |
| L10 | How many open P1 issues are there? | L3's form with `limit=0` | yes: the description says `N of M` |

For L6, the catalog's `updated` column is the store's modification time. In a fresh clone every
file has the clone's time, so front-matter `created` is the reliable date.

## 2. A general agent memory (OpenViking style)

As I understand OpenViking, it organises an agent's memories, resources and skills as a file
system under `viking://` URIs, loads context in tiers, finds the relevant directory before drilling
into it, retrieves semantically, and extracts long-term memories from sessions. The tiers are L0
(an abstract of about 100 tokens), L1 (an overview) and L2 (the full document). The questions such
a system answers:

| # | Question | Expression | Status |
|---|---|---|---|
| G1 | What do I know about X? | `search-X` over the memory folder, ranked | yes, lexical. Semantic search is `external-index-sync`'s job |
| G2 | My memories of kind K (preferences, entities, events, cases) | A folder scope (`-R-key/mem/user/preferences/-/…`) or `kind:preference` after `field_extract` | yes |
| G3 | The most recent memories about X | `created:>…`, then `sort` | Filtering yes; ordering needs `sort` ([`record-filter-commands`](../record-filter-commands/)) |
| G4 | Tiered loading: abstracts of the top hits, then one document in full | `search` returns `title` (L0), `description` and `excerpt` (L1) and `key`; L2 is `-R/<key>` | yes, **if** title and description are filled. Plain Markdown without front-matter has neither. See P3 |
| G5 | Which folder is most about X? (directory-first retrieval) | Search, then group the hits by `parent` | Needs grouping. That is polars today (`ns-pl`), so it stays out of scope |
| G6 | Have I stored this already? | As L4 | yes |
| G7 | Ranking by importance or recency decay | — | Out of scope: a scoring policy, not a search primitive |
| G8 | Things similar to this memory | — | `external-index-sync` (vectors) |
| G9 | The passage that answers the question, not the whole file (retrieval-augmented generation) | — | **no**: P2 |

## 3. The tools a coding agent uses here

These are the operations used in this session, and in most sessions on this repository:

| Tool use | Expression | Status |
|---|---|---|
| `Glob specs/**/*.md` | `catalog`, then `key:specs/*.md` (`*` crosses `/`) | yes |
| `Grep -l pattern`: which files contain a literal | `search-<literal>-f-0` over `catalog-t-t` | yes |
| `Grep -l` with a **regex** (`fn \w+_asset_info`) | — | **no**: P1 |
| `Grep -n`: the matching *lines*, with line numbers | — | **no**: P2 |
| `Grep -A3 -B3`: lines around each match | Line records, then a slice around the match | partly, with P2 and `ns-rec/slice` |
| `Grep -c`: a count per file | — | no. It is rare, and the result description gives the total |
| `Read` with `offset`/`limit`, `sed -n a,bp`: part of a file | Line records for one key, then `ns-rec/slice` | with P2 |
| `head -12`: just the front-matter | `field_extract` | yes |
| `grep … specs/index.csv \| cut -d, -f1-4` | `field_extract`, filter, then `ns-rec/select_columns` | yes |
| `git log` / `git diff` | — | Not a store question; out of scope |

**The two real gaps are regular expressions and sub-file results** (lines and sections). Together
they are most of what `Grep` does, and the main thing a search over whole files cannot do.

## 4. Can everything be expressed without the `ns-rec` filters?

Yes, except ordering. Every filter in `record-filter-commands` is a search expression run with
`rank=false` and `limit=0`:

| Filter command | The same as a search expression |
|---|---|
| `eq-phase-architecture` | `phase:architecture` |
| `startswith-filename-phase1` | `filename:phase1*` |
| `isin-priority-P0-P1` | `(priority:P0 \| priority:P1)` |

The filters only save escaping, which matters to a person typing a URL or writing a recipe, and not
to an agent whose tool builds the query. `sort` is the one thing the search syntax cannot express,
and it belongs with the filters.

## 5. Proposals

**P1 — Regular expressions.**
- `/re/` is a term matched in the search fields, and `field:/re/` matches one field. This is
  Lucene's notation.
- Use the `regex` crate: its matching time is linear, so a hostile pattern cannot hang a search,
  and it builds for wasm.
- `case_sensitive=false` adds `(?i)`.
- A regex term contributes to the score through its match count, like a term.
- In a URL, `/` is `~/`, `\` is `~nbsol~` and `+` is `~nplus~`:
  `search-~/fn~.~nbsol~w~nplus~_asset_info~/` decodes to `/fn \w+_asset_info/`. That is
  unwritable by hand, so an agent tool builds it.

**P2 — `md_split`: sub-file records.**

```
async fn md_split(state, unit: String = "section" (enum: ["section", "line"]), context) -> result
```

- Input: records with `key` and `content` (from `catalog-t-t`), or a folder.
- Output: one row per section (a Markdown heading and its body) or per line.
- Columns:
  - `key`: the source;
  - `id`: `key#n`, the Id;
  - `line` and `end_line`: 1-based;
  - `heading`: the section path, `Phase 2 > Data Structures`;
  - `text`.
- What it provides:
  - `grep -n` is `catalog-t-t/md_split-line/search-~/re~/-f-0`;
  - a retrieval passage is `catalog-t-t/md_split-section/search-…`;
  - reading part of a file is `md_split-line`, then `ns-rec/slice`.
- No new search machinery: it is a record producer like `catalog`.

**P3 — L0/L1 for plain Markdown.** When `catalog` reads content (`content=true`), a Markdown entry
whose metadata has no title takes the first `# ` heading as its `title`. One rule, and G4 then works
for notes written without front-matter. A `description` from the first paragraph is possible, but
less reliable, and is left out.

## 6. Simplifications

The amendments of 2026-10-11 added more than these questions need. The proposed cuts:

1. **Nested structured predicates are limited to JSON.**
   - `field:(…)` keeps two uses:
     - a field-scoped search (`content:(a -b)`);
     - field tests inside JSON text, which is what `field_extract` writes for lists and for nested
       maps when `flatten` is false.
   - The explicit formats `yaml`, `frontmatter`, `fenced_yaml` and `fenced_json` (AC-23) and the
     automatic front-matter rule (part of AC-17) are dropped from the syntax.
   - Front-matter is `field_extract`'s job: it recognises the format reliably from the extension,
     and its result is cached. Parsing every document on every search duplicates it, more weakly.
2. **The trait is trimmed to what is used.**
   - `Predicate` keeps `bind`, `rank_terms`, `to_syntax` and a JSON-only `matches_value`.
   - `PredicateSyntax` keeps parsing through `TryFrom<&str>`.
   - `PredicateLanguage` (the run-time registry) waits for the dialect argument.
3. **The `ns-rec` filters** have moved to [`record-filter-commands`](../record-filter-commands/).

The resulting surface is five commands in `ns-search`: `catalog`, `commands`, `field_extract`,
`md_split` and `search` (with regex). Every question in §§1–3 marked "yes" or "P1–P3" is answered
by them alone, except ordering by a column.
