# Phase 1: High-Level Design — Agent Memory Service (MVP)

## Feature Name

Agent memory service (MVP), revision 2 (2026-10-11; revision 1 summarized in `DESIGN.md`).
Companion analyses: [survey](./memory-systems-survey.md) ·
[specification](./service-specification.md) · [workflow porting](./workflow-porting.md) ·
[tiers and storage](./tiers-and-storage.md) · [interfaces and MCP](./interfaces-and-mcp.md).

## Purpose

Give coding agents a memory they can browse in tiers, search, write safely and check — first
`specs/` and the skills, then any corpus — as Liquers commands behind an MCP server and a CLI. This
repository already runs such a memory by hand; the design turns its bookkeeping into commands and its
derived files into assets, and leaves judgement with the agent.

## Problem Example

An agent fixing a bug notices that Markdown sections cannot be addressed, and must check for an
existing issue before filing one (`DOCS_STRUCTURE_GUIDE.md` §4.8).

**Today:** it greps `specs/index.csv` for words it guesses, opens candidate issue files whole to
judge them, and, finding none, runs `python3 scripts/docs_index.py new "…" --area lib/commands`,
which repeats a word-overlap duplicate search, writes the template and regenerates the index.
Three tools, two of them hand-written Python, and several full documents read to make a yes/no
decision. An agent without a shell cannot do it at all.

**With the service:** one MCP call, `memory_new(collection="issues", title="Markdown sections cannot
be addressed", …)`, returns ranked candidates — once the search branch is merged, its
`NO-MARKDOWN-NAVIGATION-COMMANDS` — each with `title`, `description` and an excerpt, and writes
nothing. The agent reads one L1, sees a duplicate, and is done. The same check as a query
(`ns-search` registry from the search branch):

```
-R-key/specs/issues/-/ns-search/field_extract/search-markdown~.section-t-f-5-title
```

## Scope and Acceptance Criteria

Tool keys are relative to the memory root. Collections under `specs/` are writable through the
service only, except `archive`, which is mounted read-only; agent notes live in a separate mount.

- **AC-1** Tiered listing
  WHEN an agent calls `memory_list` on `specs/issues`
  THEN every entry carries `key`, `title` and `description` — from front-matter, or from the
  collection's declared fallback where front-matter has none — and no document body is returned
- **AC-2** Ranked, honest search
  WHEN an agent calls `memory_search` with `text: expiration`, `filter: -status:closed*`,
  `path: specs/issues`, `limit: 5`
  THEN at most five hits return in descending `score`, none with a `closed*` status, each with
  `key`, `title`, `description`, `excerpt` and `matched`, plus `shown` ≤ `total`
- **AC-3** Section read
  WHEN an agent calls `memory_read` with a key and `section: Problem`
  THEN only that section's text is returned, with the document's `version`
- **AC-4** Capture with duplicate check
  WHEN an agent calls `memory_new` for a collection with a title close to an existing memory's
  THEN the candidates are returned and nothing is written; with `force` the memory is created with a
  derived id, the collection's template, schema-valid fields and `status: draft`
- **AC-5** Schema refusal
  WHEN a write sets a field to a value outside the collection's vocabulary, or creates a Markdown
  memory without `title` or `description`
  THEN the write is refused with a message naming the field, and nothing changes
- **AC-6** Precondition
  WHEN `memory_write` or `memory_edit` carries an `expected_version` that is not the current one
  THEN the write is refused and the memory is unchanged
- **AC-7** Write log and stamps
  WHEN any write succeeds through the service
  THEN `updated` (and `created`, on creation) is stamped in the front-matter, and one line —
  operation, key, versions, reason — is appended to the month's file under `_log/`
- **AC-8** Supersession
  WHEN a memory is written with `supersedes: <old key>`
  THEN the old memory gains `superseded_by` and is omitted from `memory_list` and `memory_search`
  unless `include_superseded` is set
- **AC-9** Check matches the oracle
  WHEN `liquers-mem check specs` runs on this repository and on a fixture with seeded violations
  THEN it reports the same error set (rule, file) as `scripts/docs_index.py --check` for every rule
  the schema expresses, and lists the rules it does not
- **AC-10** Index as an asset
  WHEN the `specs/index.csv` recipe is evaluated twice with one issue's front-matter changed in
  between, and twice with nothing changed
  THEN each result's bytes equal `docs_index.py`'s output, and the asset's `version` changes in the
  first case and not in the second
- **AC-11** Front-matter projection
  WHEN `key/info` is asked for a Markdown memory, the file is edited outside the service, and
  `key/info` is asked again — with the asset loaded in memory and without
  THEN `title` and `description` equal the file's front-matter every time
- **AC-12** Search never evaluates
  WHEN a listed or searched folder contains a key only a recipe declares
  THEN it appears with status `Recipe` and its recipe is not run
- **AC-13** Read-only mount
  WHEN an agent lists and reads `specs/archive`, then tries to write into it
  THEN the reads create no sidecar files and the write is refused
- **AC-14** MCP discovery and use
  WHEN an MCP client starts `liquers-mem mcp` over stdio and lists tools
  THEN the nine `memory_` tools of `interfaces-and-mcp.md` §4.1 are offered with argument schemas
  as tabulated there, and a `memory_search` call returns structured hits
- **AC-15** Orientation
  WHEN an agent calls `memory_overview`, then `memory_describe` for `issues`
  THEN it receives the memory's L0/L1, its collections with counts and the latest log lines; then
  the collection's fields, vocabularies and template
- **AC-16** Edits
  WHEN an agent replaces a unique string, appends a `## History` row, or sets `status` along an
  allowed transition — and then sets it along a forbidden one
  THEN the first three succeed (AC-7 applies) and the last is refused, naming the transition
- **AC-17** Fallback tiers
  WHEN a Markdown memory has no front-matter, and a folder has a `README.md` with front-matter
  THEN the memory's `title` is its first heading and `description` its first paragraph, and the
  folder's tiers are the README's

Non-goals: identity, sessions and ACL (`CORE-SESSION-AND-KEY-ACL`); a language model inside the
service; semantic search; `memory_forget` and the archive convention (NEXT); the general Liquers
MCP (only kept combinable, `interfaces-and-mcp.md` §6); git commits by the service; retiring
`docs_index.py` before AC-9 and AC-10 pass.

## Core Interactions

- **Query:** no grammar change.
- **Store:** a front-matter store adapter (`tiers-and-storage.md` §2 C); a read-only mount.
- **Commands:** new `ns-mem` (`describe`, `similar`, `new`, `write`, `edit`, `set_field`,
  `append_row`, `check`, `render_index`, `section`), built on `ns-search`.
- **Assets:** writes go through the asset manager; recipes for indexes and folder tiers.
- **Web/API:** the existing assets API; MCP over HTTP is NEXT (question 5).

## Crate Placement

`liquers-lib` (`src/mem/`, feature `mem` implying `records`): `ns-mem` and the front-matter store,
beside `ns-search`. New `liquers-mcp` (depends on `liquers-core`): the MCP engine. New `liquers-mem`:
the binary, over `liquers-lib`, `liquers-axum` and `liquers-mcp`.

## Documentation Intent

- Reference: new `specs/reference/AGENT_MEMORY_SERVICE.md` — layout, document and schema format,
  tiers, tools and CLI. No existing reference owns it.
- Guide: new skill `.claude/skills/liquers-memory/` for agents using the service.
- Other documents: none.
- Documents to update: `CLAUDE.md` project structure and dependency flow (two crates);
  `DOCS_STRUCTURE_GUIDE.md` §3 (an `area` for MCP) and §4.8 (the fast path becomes the tool);
  `specs/README.md` capability map.

## Design Dependencies

- **Requires** `store-and-asset-search` (on branch `claude/relaxed-brahmagupta-1tadmg`, unmerged;
  `ns-search`, `SEARCH_GUIDE.md`) and `STORE-NO-READ-ONLY-ADAPTER` (AC-13).
- **Overlaps** `NO-MARKDOWN-NAVIGATION-COMMANDS` (search branch; AC-3, AC-17),
  `STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO` (folder tiers), `ARGUMENT-INFO-HAS-NO-DESCRIPTION` (tool
  schemas), `CORE-METADATA-NO-APPLICATION-ATTRIBUTES`, `STORE-WRITE-HAS-NO-PRECONDITION` (AC-6 is
  single-process until it is fixed).
- **Required by** `external-index-sync` (search branch) for semantic memory search.

## Open Questions

**Blocking**

1. *Sequencing.* Phase 2 starts when `store-and-asset-search` is on `main`. *Recommended: yes.*
2. *`STORE-NO-READ-ONLY-ADAPTER`* fixed first as its own change, or wrapped here? *Recommended: first.*

**Open design**

3. *L1 for the existing corpus.* No `specs/` document has `description:` front-matter today. Rely on
   per-collection fallbacks (an issue's first `Problem` paragraph, a design's `Purpose`), backfill
   descriptions once, or both? *Recommended: fallbacks now (AC-1), backfill by agents over time.*
4. *Tier storage* — front-matter projected into metadata (C), or reached only through
   `field_extract`? And does setting `description` through the assets API rewrite the front-matter
   or get refused? *Recommended: C, and rewrite* (`tiers-and-storage.md` §3).
5. *HTTP MCP (`serve`) in the MVP?* *Recommended: NEXT*; stdio and CLI first.

**Proposed resolutions** (accept or amend)

6. Agent-written memory in a separate writable mount; promotion into `specs/` by filing.
7. Schema language: a small own YAML, exported as JSON Schema for MCP.
8. The oracle covers `index.csv` and `--check`; `index.md` and README blocks follow later.
9. Nine tools; `memory_forget` waits for the archive convention.

## References

Source `AGENT-MEMORY-SERVICE`; prerequisites in `service-specification.md` §5; systems in the survey.
