# Phase 1: High-Level Design — Agent Memory Service (MVP)

## Feature Name

Agent memory service (MVP), revision 5 (2026-10-17). Revision 4 reconciled it with the merged
`store-adapters` and the approved Phases 1–2 of `liquers-mustache`, then split it: templates, indexes
and context packs moved to [`agent-memory-derived`](../agent-memory-derived/). Revision 5 aligns the
text with the decisions approved in Phase 2 (its changes 1–10): L0/L1 through a configurable bridge,
Liquers-style commands, `memory_query`, serialized results, the migration guide. Earlier revisions
are in `DESIGN.md`.
Companion analyses: [survey](./memory-systems-survey.md) ·
[specification](./service-specification.md) · [workflow porting](./workflow-porting.md) ·
[tiers and storage](./tiers-and-storage.md) · [interfaces and MCP](./interfaces-and-mcp.md) ·
[brainstorm: workflow improvements](./brainstorm-workflow-improvements.md) ·
[template language evaluation](./template-language-evaluation.md).

## Purpose

An MVP of a **competitive agent memory system**: a memory agents browse in tiers, search, write
safely and check — served as Liquers commands behind an MCP
server and a CLI. It is **configurable**: one configuration document (stores, collections, tiers,
views, tools) shapes it like this repository's manual system over the unchanged `specs/`, or like
OpenViking's context database, on local directories or remote stores such as S3. The bookkeeping
becomes commands, derived files become assets, judgement stays with the agent. What is derived from
the memory — rendered templates, indexes, context packs — is the follow-on design
[`agent-memory-derived`](../agent-memory-derived/).

## Problem Example

An agent fixing a bug notices that Markdown sections cannot be addressed, and must check for an
existing issue before filing one (`DOCS_STRUCTURE_GUIDE.md` §4.8).

**Today:** it greps `specs/index.csv` for words it guesses, opens candidate issue files whole to
judge them, and, finding none, runs `python3 scripts/docs_index.py new "…" --area lib/commands`,
which repeats a word-overlap duplicate search, writes the template and regenerates the index.
Three tools, two of them hand-written Python, and several full documents read to make a yes/no
decision. An agent without a shell cannot do it at all.

**With the service:** one MCP call, `memory_new(collection="issues", title="Markdown sections cannot
be addressed", …)`, returns ranked candidates — first among them the existing
`NO-MARKDOWN-NAVIGATION-COMMANDS` — each with `title`, `description` and an excerpt, and writes
nothing. The agent reads one L1, sees a duplicate, and is done. The same check as a query:

```
-R-key/specs/issues/-/ns-search/field_extract/search-markdown~.section-t-f-5-title
```

## Scope and Acceptance Criteria

The configuration profiles are sketched in the brainstorm §2. Tool keys are relative to the memory
root. Collections under `specs/` are writable through the
service only, except `archive`, which is mounted read-only; agent notes live in a separate mount.

- **AC-1** Tiered listing
  WHEN an agent calls `memory_list` on `specs/issues`
  THEN every entry carries `key`, `l0` and `l1` — by default the front-matter's `title` and
  `description`, or the declared fallback where front-matter has none (Phase 2 change 8) — and no
  document body is returned
- **AC-2** Ranked, honest search
  WHEN an agent calls `memory_search` with `text: expiration`, `filter: -status:closed*`,
  `path: specs/issues`, `limit: 5`
  THEN at most five hits return in descending `score`, none with a `closed*` status, each with
  `key`, `l0`, `l1`, `excerpt` and `matched`, plus `shown` ≤ `total` (as a note beside the table)
- **AC-3** Section read
  WHEN an agent calls `memory_read` with a key and `section: Problem`
  THEN only that section's text is returned, with the document's `version`
- **AC-4** Capture with duplicate check
  WHEN an agent calls `memory_new` for a collection with a title close to an existing memory's
  THEN the candidates are returned and nothing is written; with `force` the memory is created with a
  derived id, the collection's template file copied as it is, schema-valid fields and
  `status: draft` (rendering the template is `agent-memory-derived` AC-1)
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
  WHEN `liquers-agent check specs` runs on this repository and on a fixture with seeded violations
  THEN it reports the same error set (rule, file) as `scripts/docs_index.py --check` for every rule
  the schema expresses, and lists the rules it does not
- ~~**AC-10** Index as an asset~~ — moved to [`agent-memory-derived`](../agent-memory-derived/) AC-2 on 2026-10-17
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
  WHEN an MCP client starts `liquers-agent mcp` over stdio and lists tools
  THEN the nine basic `memory_` tools of Phase 2's tool table and `memory_query` are offered with
  argument schemas as tabulated there, and a `memory_search` call returns its hits as a table
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

- **AC-18** The "liquers-specs" profile
  WHEN the service starts with a configuration mounting this repository's `specs/` and skills
  unchanged
  THEN AC-1 to AC-17 (AC-10 excepted) hold without moving or rewriting any file
- **AC-19** An OpenViking-shaped profile
  WHEN the service starts with a configuration declaring category collections (profile,
  preferences, entities, events) and folder overviews, and no code change
  THEN `memory_overview`, `memory_list`, `memory_new` and `memory_search` work over that layout
- **AC-20** A remote mount
  WHEN a collection is mounted from an S3-compatible store (OpenDAL; an in-process or local
  S3-compatible endpoint in tests)
  THEN writes and reads through the tools behave as on a local mount, and the memory survives a
  restart of the service
- ~~**AC-21** Context pack~~ — moved to [`agent-memory-derived`](../agent-memory-derived/) AC-3 on 2026-10-17
- ~~**AC-22** `index.md` as a recipe~~ — moved to [`agent-memory-derived`](../agent-memory-derived/) AC-4 on 2026-10-17

Non-goals: identity, sessions and ACL (`CORE-SESSION-AND-KEY-ACL`); a language model inside the
service; rendered templates, indexes and context packs (`agent-memory-derived`); semantic search and vector indexes (brainstorm §5.3); `memory_forget` and the archive convention (NEXT); the general Liquers
MCP (only kept combinable, `interfaces-and-mcp.md` §6); git commits by the service; retiring
`docs_index.py` before AC-9 here and `agent-memory-derived` AC-2 pass.

## Core Interactions

- **Query:** no grammar change.
- **Store:** no new store adapter; one new read handler, `markdown_tiers` (Phase 2 change 4). Mounts use what `store-adapters` shipped: `read_only: true`
  (AC-13), and a `metadata` section whose read and write chains include `-store_config-front_matter`
  and `-store_config-front_matter_sync` (AC-11; `liquers-lib`, feature `records`). Write-back
  happens in `set_metadata` only; in `set` the written bytes are the truth, which suits writes that
  carry a whole document.
- **Commands:** new `ns-mem` (`overview`, `describe`, `list`, `search`, `read`, `similar`, `new`,
  `write`, `edit`, `check`), built on `ns-search`; field setting, row appending and section reads are
  arguments of `edit` and `read` (Phase 2 changes 1–2). Commands are named by `CommandKey` Display; a command that resolves relative keys takes `cwd` as
  an explicit default-link argument, never from `Context` (`COMMAND_REGISTRATION_GUIDE.md`).
- **Configuration:** the existing store-router configuration with its `metadata` sections,
  extended with collections, tiers, views and tool sets.
- **Assets:** writes go through the asset manager; recipes for indexes and folder tiers.
- **Web/API:** the existing assets API; MCP tool arguments map onto argument overrides; MCP over
  HTTP is NEXT (question 5).

## Crate Placement

`liquers-lib` (`src/mem/`, feature `mem` implying `records`): `ns-mem` and the front-matter store,
beside `ns-search`. New `liquers-mcp` (depends on `liquers-core`): the MCP engine. New
`liquers-agent` (named 2026-10-15, replacing the working name `liquers-mem`; named for its
audience so it can later carry the workbench tool sets of `AGENT-WORKBENCH-SURFACE` without a
rename): the binary, over `liquers-lib`, `liquers-store` and `liquers-mcp` (`liquers-axum` arrives with
`serve`, which is NEXT; Phase 2 change 6).

## Documentation Intent

- Reference: new `specs/reference/AGENT_MEMORY_SERVICE.md` — layout, document and schema format,
  tiers, tools and CLI. No existing reference owns it.
- Guide: new skill `.claude/skills/liquers-memory/` for agents using the service; new
  `specs/guides/AGENT_MEMORY_MIGRATION_GUIDE.md` (Phase 2 change 10).
- Other documents: none.
- Documents to update: `CLAUDE.md` project structure and dependency flow (two crates);
  `DOCS_STRUCTURE_GUIDE.md` §3 (an `area` for MCP) and §4.8 (the fast path becomes the tool);
  `STORE_CONFIG_GUIDE.md`, `COMMAND_REGISTRATION_GUIDE.md`, `PROJECT_OVERVIEW.md` (Phase 2);
  `specs/README.md` capability map.

## Design Readiness

Consistency pass before the Phase 4 gate, 2026-10-17. **Blocking: none. Needs decision: none.**
Open risks, each with its test and fallback:
- `s3s-fs` as the in-process S3 endpoint for AC-20 is unchecked against OpenDAL's client
  (`s3_mount_round_trip`; fallback: `#[ignore]` behind `LIQUERS_TEST_S3_ENDPOINT`, recorded as a
  deviation).
- `check` parity with `docs_index.py` depends on mapping its messages to rules
  (`check_matches_docs_index_oracle`).
- Listing cost of `storage: none` with `front_matter` over 334 issues (`storage_none_listing_cost`).
- `read`'s version must equal `commit`'s (`read_version_matches_commit_version`).

## Design Dependencies

- **Requires** `store-and-asset-search` (merged; `ns-search`, `SEARCH_GUIDE.md`),
  `axum-query-named-arguments` (merged; argument overrides), `store-adapters` (**complete**, PR #137:
  read-only mount AC-13, `front_matter` handlers AC-11),
  and the minimum of `NO-MARKDOWN-NAVIGATION-COMMANDS` (section by heading path, AC-3).
- **Required by** [`agent-memory-derived`](../agent-memory-derived/) (split off 2026-10-17, decision
  12), which takes templates, indexes and packs and the dependencies they bring
  (`liquers-mustache`, `record-filter-commands`, `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES`,
  Rust item records).
- **Overlaps** `STORE-METADATA-HANDLING-NOT-COMPOSABLE` (decision 4; the front-matter adapter is one
  instance), `NO-MARKDOWN-NAVIGATION-COMMANDS` (AC-3, AC-17),
  `CORE-METADATA-NO-APPLICATION-ATTRIBUTES` (`front_matter` projects only `title` and
  `description`; other fields are read as `field_extract` records),
  `RECIPE-OVERRIDE-ONLY-LAST-ACTION` (an MCP tool's arguments override the last action of its
  declared query only, so each tool's query ends in the command that takes them), `STORE-WRITE-HAS-NO-PRECONDITION` (AC-6 is
  single-process until it is fixed).
- **Feeds** `external-index-sync`, where semantic memory search belongs.

## Open Questions

Answered by the user on 2026-10-12; the answers are decisions of this phase.

1. ~~*Sequencing.*~~ Resolved: `store-and-asset-search` is merged.
2. **`STORE-NO-READ-ONLY-ADAPTER` is fixed first** — done: `ReadOnlyStore` and `read_only: true`
   shipped with [`store-adapters`](../store-adapters/) (PR #137, merged 2026-10-17).
3. **L1 for the existing corpus:** per-collection fallbacks now (AC-1, AC-17); descriptions are
   backfilled by agents over time.
4. **Tiers:** a store adapter projects front-matter into metadata, and setting `title` /
   `description` through metadata rewrites the front-matter. This adapter and the read-only one are
   instances of a general, configurable metadata pipeline (create, enhance, store, policy), filed as
   `STORE-METADATA-HANDLING-NOT-COMPOSABLE` and designed in [`store-adapters`](../store-adapters/)
   (2026-10-13). It shipped: the front-matter adapter is the `front_matter` / `front_matter_sync`
   handler pair in `liquers-lib`, so this design configures it rather than building it.
5. **HTTP MCP (`serve`)** is NEXT; stdio MCP and the CLI first.
6. **Rust item records** (`syn`) are in the MVP; references and call graphs (SCIP) later. Since the
   split they serve only context packs, so they moved with them to `agent-memory-derived`.
7. **Templates must understand queries.** The language is
   [`liquers-mustache`](../liquers-mustache/) (crate `liquers-template`, Phases 1–2 approved
   2026-10-17). Moved to `agent-memory-derived` with the split, where the interim engine is dropped
   pending confirmation.
8. **Agent-written memory** is a separate writable mount — committed `notes/` by default, S3 when the
   network policy allows. A **separate repository** works today as a second clone mounted with the
   `filesystem` type (in a cloud session: added to the session and cloned; git push by the agent).
   OpenDAL has no generic git service; its `github` service reads and writes through the GitHub
   Contents API (one commit per write; needs API access, which this container's policy blocks). A git
   store over a local clone (commit on write) would be a new backend — NEXT.
9. **Schemas** are a small YAML format of our own; the tool converts between formats (YAML, JSON,
   TOML, JSON Schema for MCP) itself.
10. **Indexes are recipes over separate, simple commands** — moved to `agent-memory-derived`.
11. **Ten tools**, `memory_forget` later. Here: nine basic tools plus `memory_query` (Phase 2 change
    7); `memory_context` moved to `agent-memory-derived`.
12. **Split into two designs** (decided 2026-10-17). This design keeps the memory core, the MCP
    server, tiers and profiles (execution plan memory A, B, C), whose inputs are on `main`.
    [`agent-memory-derived`](../agent-memory-derived/) takes templates in `memory_new`, indexes and
    context packs (memory D, E, F; former AC-10, AC-21, AC-22 and AC-4's template step).

## References

Source `AGENT-MEMORY-SERVICE`; prerequisites in `service-specification.md` §5; systems in the survey.
