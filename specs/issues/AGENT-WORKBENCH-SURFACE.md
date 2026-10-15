---
id: AGENT-WORKBENCH-SURFACE
kind: feature
title: Agents get only memory tools — no general surface to evaluate, validate, inspect or propose
status: draft
priority: P1
complexity: L
area: [lib/commands, core/commands, py, docs]
design:
created: 2026-10-15
github:
---
## Problem

The agent-memory MVP (`design/agent-memory-mvp/`) gives an agent one tool set: `memory_*`. The
direction it serves — Liquers as an **agent workbench**, where agent tool calls are validatable
before execution, cached across sessions, and provenance-tracked after
(`reference/VISION.md` §The agent workbench) — needs a *general* surface the MVP deliberately does
not design (`interfaces-and-mcp.md` §6: "a different product… not designed here"). The MVP's
engine makes it cheap (a tool set is data), but nobody has specified the tool set, so it will not
exist by default.

**Example.** An agent connected to the memory MCP server is asked to produce a filtered report.
The pipeline exists as registered commands — the query
`-R/data/sales.csv/-/ns-rec/to_record/eq-region-EU/report.html` is valid against the committed
registry (checked with `liquers-validate`) — but the agent has no tool to validate it, none to
evaluate it, and none to discover that `ns-rec` has an `eq` command at all. It falls back to a shell
or a code sandbox: the result is uncached, has no asset metadata, and leaves no provenance. When the
registry lacks a verb the agent needs (say, a domain-specific normalization), there is no sanctioned
way to propose one: the agent either gives up or writes side-band code outside the registry's
governance.

## Impact

The workbench story — "the missing middle between fixed tool lists and sandboxed arbitrary code" —
remains a vision statement with no runnable artifact. Agents using the memory service still do
their *computation* outside Liquers, so the cache, validation and provenance benefits stop at
memory operations. The follow-on features in
`design/agent-memory-mvp/workbench-direction.md` (W1–W4) have no issue to start from. Workaround:
an agent with a shell can call the `liquers-validate` CLI and the assets HTTP API by hand, which is
exactly the "teach the agent to build URLs" interface the MVP's interface analysis rejected.

## Expected behaviour

A continuation of the memory system, scoped to realistic goals with minimum additional
dependencies (the MVP engine itself; `liquers-py` command registration and the query-UI prototype
are accepted external dependencies, nothing else):

- **G1 General tool set** (`liquers_` prefix, a tool-set declaration on the MVP engine —
  `interfaces-and-mcp.md` §6 prices this at "only its tool-set declaration"): evaluate a query;
  validate a query **without executing it**; discover commands (over `ns-search/commands` /
  `CommandMetadataRegistry`); browse store keys and asset status. Restricted context applies
  (`COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`).
- **G2 Inspectable plans**: the validate tool returns the resolved plan in a stable, documented
  JSON shape (the `liquers-validate` envelope already exists) that the visual query representation
  (prototype, separate repository) can render — so a human can inspect and modify an
  agent-proposed query. The UI itself is out of this issue's scope; the stable projection is in.
- **G3 Toolsmithing v0**: an agent can *propose* a command — Python once `liquers-py` implements
  the integration guide's `COMMAND` feature (`LANGUAGE-INTEGRATION_GUIDE.md`;
  `COMMAND_DECLARATION.md` is the language-neutral declaration) — into a **staging namespace**
  that is disabled unless explicitly enabled, carrying provenance (proposer, session, date) in its
  metadata. Promotion into a governed namespace is a human act (move + registry regeneration);
  no automatic promotion of any kind.
- **G4 Receipts**: an evaluation answered through G1 reports whether it was a cache hit and links
  the asset's metadata/provenance, so "what did the agent compute, from what" is answerable
  per call from data that already exists.

Out of scope, recorded so the design does not drift: multi-user enforcement
(`CORE-SESSION-AND-KEY-ACL`), cost accounting, audit-pack reports (template system), anything
hosted. More than one route exists for G3's staging (namespace vs realm vs separate registry
overlay) — the design decides, not this issue.

A design is needed (complexity L) and is deliberately deferred: it builds on the MVP's tool-set
engine, whose own design is `in_review` and unimplemented, so a Phase 2 here could not name real
symbols yet. Start the design when `AGENT-MEMORY-MVP` reaches implementation. Direction map:
`design/agent-memory-mvp/workbench-direction.md` (this issue is W1–W4 of its ladder, trimmed).

## Discovery

Business/vision discussion of 2026-10-15 (the agent-workbench direction added to
`reference/VISION.md` and `workbench-direction.md`). Triage: `AGENT-MEMORY-MVP` is the strong
candidate but explicitly excludes this surface from its scope, so this is filed as its
continuation and dependency rather than a scope extension; Python command registration is prior
art (`COMMAND` feature in the integration guide, `COMMAND-DECLARATION` design complete for the
declaration format) and a dependency, not part of this issue.
