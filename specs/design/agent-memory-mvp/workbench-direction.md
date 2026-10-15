# The agent workbench — direction beyond the memory MVP

Companion to [Phase 1](./phase1-high-level-design.md) and
[`interfaces-and-mcp.md`](./interfaces-and-mcp.md), written 2026-10-15. It records the product
direction the memory MVP is the first step of, and the features planned on top of it, so that the
MVP does not foreclose them. **Nothing here adds scope to the MVP.** Each item below becomes its
own issue and design when work starts; this document is the map, not the commitment. The vision
framing lives in [`reference/VISION.md`](../../reference/VISION.md) §The agent workbench.

## 1. The reading

The memory service is the first application of a more general product: **Liquers as the layer
where agents do deterministic work** — the *agent workbench*. The argument: an agent's tools today
are either a fixed tool list (safe, rigid) or arbitrary code in a sandbox (flexible, unauditable).
A Liquers query is the middle — compositional like code, and validatable before execution, cached
across sessions and agents, provenance-tracked after the fact.

`interfaces-and-mcp.md` §6 already made the three decisions that keep this open: the MCP plumbing
lives in a generic `liquers-mcp` crate; a tool is a *declared query* generated from
`CommandMetadata`; a server hosts several named tool sets. The memory MVP builds the engine; the
workbench is further tool sets and the governance around them.

## 2. What the MVP must not foreclose

Checks against the current design, to be re-verified at each phase gate:

- **Tool-set engine stays memory-agnostic.** No `ns-mem` semantics hard-coded in `liquers-mcp`;
  the memory tools remain one declared tool set among possible others (§6 decision 2–3 — already
  the design's position).
- **Resource and prompt names carry the tool-set prefix**, so a `liquers_` general set can join a
  `memory_` set in one server without collision (§6 decision 3 covers tools; resources and prompts
  should follow the same rule).
- **Validation stays a first-class operation.** The engine's path from tool call to query must
  keep a no-execution validation entry point, since W1 exposes it as a tool.

## 3. The ladder — features on top of the memory system

Ordered roughly by dependency, not by date. Status of all: **planned** (recorded here; no design).

| # | Feature | What it is | Depends on |
|---|---|---|---|
| **W1** | **General tool set** | `liquers_` tool set: evaluate a query, validate a query (no execution), discover commands (`ns-search/commands`), browse stores/assets. Per §6, "costs only its tool-set declaration" once the MVP engine exists. | memory MVP engine |
| **W2** | **Python command registration** | Register a command from a Python function through `liquers-py` — the single gate feature for both the data-science audience and W3. | PyO3 work in `liquers-py` |
| **W3** | **Toolsmithing** | An agent *proposes* a command in Python; it lands in a staging namespace/realm with provenance (who, when, from which session), runs only where that realm is enabled, and a person promotes it into the governed registry. Versioned via the existing `version:` machinery. *Agents coin verbs; people govern the vocabulary.* | W2; realms; registry export |
| **W4** | **Query inspection UI** | The visual representation of a query (prototype in a separate repository): render an agent-proposed query as an editable structure so a human can inspect and modify it before approving or re-running it. | UI roadmap item; `liquers-validate` |
| **W5** | **Governance metadata** | Safety/cost hints on commands (destructive, expensive, volatile — partially existing) surfaced in tool declarations; per-tool-set restrictions on namespaces and key prefixes. Multi-user enforcement waits on `CORE-SESSION-AND-KEY-ACL` (out of scope for the MVP, per `DESIGN.md` §Out of scope). | command metadata; ACL for multi-user |
| **W6** | **Shared cache and cost accounting** | The asset store as a cross-agent, cross-session computation cache, with per-asset cost metadata so "the agent re-derived this nine times" becomes visible and then impossible. | MVP; metadata |
| **W7** | **Audit pack** | A rendered "what did the agent do" report — assets produced, queries run, dependencies, commands proposed — assembled by the template system from provenance already recorded. | template system (`template-language-evaluation.md`) |

## 4. Non-goals of this direction

- Liquers does not become an orchestration framework: no reasoning loops, no prompt management, no
  agent control flow. It is the layer *under* those (LangChain/LangGraph, agent SDKs), reached
  over MCP.
- The workbench is a wedge, not a rebrand: the substrate identity in `VISION.md` stays primary.
