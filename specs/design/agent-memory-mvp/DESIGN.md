---
id: AGENT-MEMORY-MVP
kind: design
title: MVP for an agent memory service on the liquers stack
workflow: liquers-project
status: in_review
phase: high-level
area: [axum, lib/commands, core/store, docs]
gh_pr: []
issues: [AGENT-MEMORY-SERVICE]
affects_docs: []
created: 2026-09-15
superseded_by:
---
# Agent memory service (MVP) — design tracking

**Created:** 2026-09-15

## Phase Status

- [ ] Phase 1: High-Level Design
- [ ] Phase 2: Solution & Architecture
- [ ] Phase 3: Examples & Testing
- [ ] Phase 4: Implementation Plan
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

## Links

- [Phase 1](./phase1-high-level-design.md)
- [Survey](./memory-systems-survey.md) · [Specification](./service-specification.md) · [Workflow porting](./workflow-porting.md) · [Tiers and storage](./tiers-and-storage.md) · [Interfaces and MCP](./interfaces-and-mcp.md)
- [Phase 2](./phase2-architecture.md)
- [Phase 3](./phase3-examples.md)
- [Phase 4](./phase4-implementation.md)
- [Phase 5](./phase5-documentation.md)
