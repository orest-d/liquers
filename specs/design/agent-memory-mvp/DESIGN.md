---
id: AGENT-MEMORY-MVP
kind: design
title: MVP for an agent memory service on the liquers stack
workflow: liquers-project
status: in_review
phase: implementation
area: [lib/commands, core/store, docs]
gh_pr: []
issues: [AGENT-MEMORY-SERVICE]
affects_docs: []
created: 2026-09-15
superseded_by:
---
# Agent memory service (MVP) — design tracking

**Created:** 2026-09-15

## Phase Status

- [x] Phase 1: High-Level Design — **approved 2026-10-17** (revision 4, after the split)
- [x] Phase 2: Solution & Architecture — **approved 2026-10-17** (decision 1 revised; decisions 2–9 settled at the gate)
- [x] Phase 3: Examples & Testing — **approved 2026-10-17** (with the migration guide draft)
- [ ] Phase 4: Implementation Plan — written 2026-10-17, in review
- [ ] Phase 5: Documentation
- [ ] Implementation Complete

## Revision 2 — 2026-10-11

Phase 1 was rewritten from a second brief: specify what an agent memory service must do, survey the
established systems (mem0, the LLM-wiki pattern, OpenViking and others), analyse how this
repository's own skills, documentation system, issues and Python scripts map onto it, decide how
the L0/L1/L2 tiers are stored, and decide between CLI, server and MCP. The analysis is five
companion documents:

- [`memory-systems-survey.md`](./memory-systems-survey.md) — thirteen systems, their data models,
  write and read paths and MCP tools, and the 22 services a memory system provides (S1–S22).
- [`service-specification.md`](./service-specification.md) — eight principles and the capability
  list (read, write, maintain, operate) at MVP / NEXT / OUT levels.
- [`workflow-porting.md`](./workflow-porting.md) — `specs/` as an existing memory, and which
  script or convention becomes which `ns-mem` command.
- [`tiers-and-storage.md`](./tiers-and-storage.md) — metadata, front-matter, a front-matter store
  that projects one into the other, derived tiers, and OpenViking's tier files; recommends the
  bridge.
- [`interfaces-and-mcp.md`](./interfaces-and-mcp.md) — one binary with MCP (stdio first), HTTP and
  CLI faces; the nine-tool memory MCP; how a general Liquers MCP combines with it.

What changed from revision 1, and why:

1. **The blocker is gone.** `AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED` was closed by
   `axum-assets-endpoints`; revision 1's dependency on it is removed.
2. **Search exists.** `store-and-asset-search` is implemented on its branch (`ns-search`), which
   answers revision 1's open question 3; this design now builds on it and waits for its merge.
3. **Tiers come from front-matter, projected into metadata** (revision 1: metadata only). Revision
   1's open question 2 — what populates `title` and `description` for existing documents — is
   answered by the front-matter store, with per-collection fallbacks for the L1 — no `specs/`
   document has a `description:` field today.
4. **The service calls no model** (principle P2): the agent writes and consolidates; the service
   validates, logs, indexes and checks.
5. **Acceptance scenarios AC-1 to AC-17** are defined. Revision 1's open question 5 (the
   `docs_index.py` oracle) is kept, narrowed to `index.csv` and `--check` (AC-9, AC-10), and
   balanced by retrieval scenarios (AC-1 to AC-3).

Revision 1's open question 1 (an asset listing is the union of live assets, store and recipes) was
settled by `axum-assets-endpoints` and is relied on by AC-12.

## Execution plan — 2026-10-13

[`execution-plan.md`](./execution-plan.md) orders the work this design depends on into waves: small
fixes first (`STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO`, `VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT`),
the three Phase 1 gates, then three parallel tracks (store adapters, the template language, record
filters), with the memory service's core and MCP server first because they need only the read-only
adapter. It also records that `ARGUMENT-INFO-HAS-NO-DESCRIPTION` and
`STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS` were closed after the analyses were written.

## Spin-off designs — 2026-10-13

Two decisions became designs of their own, at the user's request:
[`store-adapters`](../store-adapters/) (the read-only and front-matter adapters as instances of a
configurable metadata pipeline; merges `STORE-METADATA-LAYOUT-HARDCODED-PER-STORE`,
`STORE-NO-READ-ONLY-ADAPTER`, `STORE-METADATA-HANDLING-NOT-COMPOSABLE`) and
[`liquers-mustache`](../liquers-mustache/) (the query-aware template language;
`NO-QUERY-AWARE-TEMPLATE-LANGUAGE`, with `SIMPLE-TEMPLATE-HAS-NO-COMMAND` moved there). This design
requires both.

## Phase 1 decisions — 2026-10-12

The user answered the open questions; Phase 1 records them as decisions. Read-only adapter first;
fallback summaries; the front-matter store adapter with write-back, as one instance of a general
metadata pipeline (filed `STORE-METADATA-HANDLING-NOT-COMPOSABLE`); HTTP MCP later; Rust item
records in the MVP; indexes as recipes over separate simple commands, `index.md` in scope (AC-22);
a separate repository as an extra mount; no split until Phase 1 is approved. The template question
became a language evaluation —
[`template-language-evaluation.md`](./template-language-evaluation.md) recommends reimplementing
Mustache with query tags — awaiting confirmation.

## Revision 3 — 2026-10-12

`store-and-asset-search` and `axum-query-named-arguments` (argument overrides) were merged. The
brief was sharpened: the MVP is a **competitive** agent memory system, **configurable** to serve this
repository's manual system or an OpenViking-style layout, on local or remote (S3) stores. A sixth
companion, [`brainstorm-workflow-improvements.md`](./brainstorm-workflow-improvements.md), measures
today's workflow costs, sketches the configuration profiles, walks ten workflow improvements,
compares templating (`simple_template`/`expand`, ranees' `mustache`, minijinja), ranks further
features (Markdown navigation, context packs, Rust item records via `syn`, hybrid semantic search
with vendored static embeddings, why no vector database), and checks each against the Claude Code
cloud container (network policy, build size, ephemeral state). Phase 1 gained AC-18 to AC-21
(profiles, a remote mount, context packs), a tenth tool (`memory_context`), and open questions 6–7
(Rust item records in the MVP; the template engine); the sequencing question is resolved. Filed:
`SIMPLE-TEMPLATE-HAS-NO-COMMAND`, `VALUE-TO-JSON-PANICS-ON-NON-FINITE-FLOAT`.

## Phase 1 review, 2026-10-11

Two independent reviews (conformity and records; codebase and soundness) were applied before the
gate. The findings that changed substance: the corpus has no `description:` front-matter (L1 now
comes from declared fallbacks, open question 3); the front-matter projection must happen in `get`
as well as `get_metadata`, must notice outside edits by file facts rather than the sidecar version,
and reaches folders only when wrapping the router (`STORE-ROUTER-DOES-NOT-FORWARD-ASSET-INFO`,
filed); writes go through the asset manager as volatile, payload-carrying commands; MCP argument
descriptions cannot all come from `CommandMetadata` (`ARGUMENT-INFO-HAS-NO-DESCRIPTION`); `forget`
and HTTP `serve` moved to NEXT; AC-15 to AC-17 added for orientation, edits and fallback tiers.

## Notes

Phase 1 written 2026-09-15 from a brainstorming question: what would it take to turn Liquers into
a memory system for coding agents, in the role OpenViking fills, with `liquers-axum` as the
interface, stores for storage and commands as the tool layer.

Three corrections from the first draft shaped revision 1 of Phase 1 (history; revision 2 above supersedes the third):

1. **The design structure is this skill's**, not an ad-hoc folder. The earlier 300-line "phase 1"
   was Phase 2 material wearing a Phase 1 label.
2. **`title` and `description` are already the L0 and L1 tiers.** `MetadataRecord` and `AssetInfo`
   both carry them, so tiering needs no new accessor commands — only a way to populate them for
   documents that arrive without them.
3. **The client interface is the assets API, not the store API.** Which made the blocker visible:
   six of its ten endpoints are 501 stubs
   (`AXUM-ASSETS-API-ENDPOINTS-NOT-IMPLEMENTED`, P0), `listdir` among them. Writes likewise belong
   in a command reaching the store or asset manager through `Context`, not in a raw store POST.

## Review findings carried forward

Codex reviewed the first draft on PR #70 and raised five findings. The document they were written
against was replaced, so all five threads read as outdated — but every one of them is a true
statement about the codebase, verified independently at HEAD, and Phase 2 must not rediscover them:

| Finding | Status |
|---|---|
| Mounting `specs/` through a raw `AsyncFileStore` is not read-only: `get` persists synthesized metadata, and `StoreApiBuilder` exposes `PUT`/`DELETE` on every mounted key | Filed as `STORE-NO-READ-ONLY-ADAPTER`. Blocked revision 1's "no `specs/` write path" non-goal; in revision 2 it blocks AC-13 (read-only mount) |
| `recipes.yaml` deserializes as `RecipeList`, whose root is a **mapping with a `recipes:` key**, not a bare sequence | Carried here. Phase 2 must write the mapping form |
| The store entry write is `PUT`, not `POST` | Filed as `WEB-API-SPECIFICATION-DIVERGES-FROM-IMPLEMENTATION` |
| `FullApiBuilder` does not exist; the four builders are merged by the caller | Same issue |
| The assets WebSocket defaults to `{base_path}/ws`, not `/liquer/ws/assets` | Same issue |

Three of the five came from copying forms out of `reference/WEB_API_SPECIFICATION.md` in good
faith. That the specification was wrong in each case is the finding behind the finding, and it is
why this design now verifies API shapes against the builders rather than against the reference.

## Out of scope

**Sessions and identity.** Liquers aims to be stateless in the way HTTP is
(`reference/PROJECT_OVERVIEW.md` §"Queries and Recipes define stateless executions"), and there
are at least three routes to session support — extending the existing `Session`/`User` mechanism,
carrying it in the payload, or a dedicated session folder holding sessions as assets. The routes
are recorded on `CORE-SESSION-AND-KEY-ACL`. **None of them is in scope here**: this design assumes
a single writer and no identity, and must not drift into choosing one. If the MVP turns out to
need identity, that is a finding to report, not a decision to make inside this folder.

## Decision — 2026-10-15: binary crate named `liquers-agent`

The user renamed the binary crate from the working name `liquers-mem` to **`liquers-agent`**:
named for its audience, so the workbench tool sets (`AGENT-WORKBENCH-SURFACE`) can join it later
without a rename. Applied across Phase 1 (Crate Placement, AC-11/AC-14 invocations),
`interfaces-and-mcp.md` (CLI examples), the execution plan and the brainstorm. `liquers-mcp`
(the generic engine) and the `mem` feature/`src/mem/` module in `liquers-lib` are unchanged.

## Direction note — 2026-10-15

The memory service is now read as the first application of a broader direction: Liquers as an
**agent workbench** (validated, cached, provenance-tracked tool calls over a governed registry;
`reference/VISION.md` §The agent workbench). This adds **no scope** to the MVP. It adds one
standing check at each phase gate — the MVP must not foreclose the general surface — recorded with
the planned follow-on features (general tool set, Python command registration, toolsmithing, query
inspection UI) in [`workbench-direction.md`](./workbench-direction.md). The separability itself
was already decided in [`interfaces-and-mcp.md`](./interfaces-and-mcp.md) §6.

## Revision 4 — 2026-10-17

Phase 1 reconciled with what landed since revision 3, for the Wave 1 gate 1.3 (`execution-plan.md`):

1. **`store-adapters` is complete** (PR #137). The read-only mount (decision 2) and the
   front-matter adapter (decision 4) exist as `read_only: true` and the
   `-store_config-front_matter` / `front_matter_sync` handlers; this design configures them and
   builds no store code. Write-back happens in `set_metadata` only.
2. **`liquers-mustache` Phases 1–2 are approved** (PR #139): crate `liquers-template`, namespace
   `template`, `{{$ q}}` / `{{$$ q}}`, `-template-render` and `-template-expand`, template commands as
   aliases of `-template-render`. Context packs (AC-21) and `index.md` (AC-22) become template
   commands. Decision 7 asks to confirm this and drop the interim engine.
3. **New dependency** `SEARCH-COMMANDS-RECORD-NO-DEPENDENCY-ON-READ-FILES` (D10): until it is fixed,
   indexes and packs built on `ns-search` do not recompute (AC-10, AC-21, AC-22).
4. **`record-filter-commands` is complete**; `RECIPE-OVERRIDE-ONLY-LAST-ACTION` constrains MCP tool
   queries (arguments override the last action only).
5. **Decision 12 decided by the user: two designs.** This one keeps memory A, B, C; the new
   [`agent-memory-derived`](../agent-memory-derived/) takes D, E, F — the template step of AC-4,
   AC-10, AC-21 and AC-22 (retired here, renumbered there as AC-1 to AC-4), decisions 6, 7 and 10,
   and the `memory_context` tool.

The execution plan's Wave 1 table and dependency inventory were updated to match.

## Phase 2 — 2026-10-17

Written after Phase 1 approval, then reviewed by two independent reviewers (conformity and records;
codebase and soundness). The review found four blocking problems, all fixed before the gate:

- The profile had no mount for `_log/` and `README.md`. A catch-all `memory/` mount was added.
- Writes would have left `.__metadata__` sidecars in the checkout, and stale front-matter tiers
  after them. The fix is `storage: none` on writable checkout mounts.
- AC-11, AC-12 and AC-20 had no mechanism. Each now has one.
- The resource template hard-coded `ns-mem` into the generic engine. Resources are now declared
  per tool set.

The review also led to these fixes:

- `root` is hidden from tools, so an agent cannot leave the memory.
- The schema is read through the store, not as a cached dependency.
- The order of environment building is fixed, with S3 coming from `liquers_store`'s factory.
- `mem` is a default feature, so the registry export and `liquers-validate` know `ns-mem`.
- `McpServer::plan` validates without running.
- The preflight gained eight issues.
- Six deviations from Phase 1 are listed under "Changes from Phase 1", for confirmation.

`area` drops `axum`: `serve` is NEXT, and the MVP binary does not use `liquers-axum`.

**Decision 1 revised at the gate (2026-10-17, Orest).** The draft had every command take its key as
an argument, so that a tool schema could be generated from one command. Orest asked for Liquers
style without losing generated schemas. Now the commands take their document (`-R/<key>`) or key
(`-R-key/<path>`) as state; the basic tools are still generated from command metadata, with one
key argument that `liquers-mcp` fills into the query structurally; and a read-only, allow-listed
`memory_query` tool with a query guide and a command reference is the power layer. `read` stops
being volatile. Core key placeholders (option C) are excluded for now and filed as
`RECIPE-CANNOT-PARAMETERIZE-RESOURCE-KEY`. The discussion is in
`/mnt/project-files/agent-memory-mvp/decision1-liquers-style-tools.md`.

**Decisions 2–8 at the gate (2026-10-17, Orest).** 2, 3, 4, 7 and 8 as recommended. 5 as
recommended, with an addition: L0/L1 (for agents finding a resource) are not assumed equal to
`title`/`description` (for humans, UI and reports), so a configurable bridge in `memory.yaml`
maps document parts to L0/L1, defaulting to title and description, to allow experiments. 6
changed: results go through the serialization system: record views for tables, Markdown text for
documents, JSON for write results, with Markdown as the default for agents and other formats
chosen by the query's filename. Counts and versions travel as log notes.

## Phase 3 — 2026-10-17

Reviewed by an independent reviewer. Three blocking findings were fixed: AC-14 is now tested
against the real tool set from `liquers-agent` (stdio), since `liquers-mcp` cannot see `ns-mem`;
the AC-9 oracle maps `docs_index.py` messages to rules by pattern, drops unsupported rules and
asserts they are listed; Example 1 no longer has an agent set `accepted`. Phase 2 was corrected:
notes travel as a separate content block (stderr in the CLI), so CSV and JSON results stay
parseable, and the `query` tool's format follows the filename. AC-20 runs in-process against
`s3s-fs` instead of an ignored external test.

**Migration guide (2026-10-17, Orest).** Added to the documentation plan as
`guides/AGENT_MEMORY_MIGRATION_GUIDE.md`, drafted now in `migration-guide-draft.md`: documents are
`Source` assets with no stored metadata, `ns-mem` results are never stored and recipe results are
recomputed, so an upgrade migrates nothing by itself; schema changes are found by `check`.
`memory.yaml` gains `version` (Phase 2 change 10); three tests added to Phase 3.

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Survey](./memory-systems-survey.md) · [Specification](./service-specification.md) · [Workflow porting](./workflow-porting.md) · [Tiers and storage](./tiers-and-storage.md) · [Interfaces and MCP](./interfaces-and-mcp.md) · [Brainstorm: workflow improvements](./brainstorm-workflow-improvements.md) · [Template language evaluation](./template-language-evaluation.md) · [Execution plan](./execution-plan.md) · [Workbench direction](./workbench-direction.md)
- [Phase 2](./phase2-architecture.md) · [Migration guide draft](./migration-guide-draft.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
