# Interfaces — CLI, server, MCP

Companion to [Phase 1](./phase1-high-level-design.md), written 2026-10-11. It decides how the
capabilities of [`service-specification.md`](./service-specification.md) reach an agent: as a
command-line tool, as an HTTP server, as an MCP server, or several of these; what the MCP server
offers; and how a memory-specific MCP relates to a general Liquers MCP. Only the memory MCP is
designed here; the general one is constrained only so that the two can later be combined.

## 1. Who calls, and how

| Caller | Has | Natural interface |
|---|---|---|
| Coding agent in a checkout (Claude Code, Codex, Copilot) with a shell | Bash, file tools, MCP client | MCP over **stdio** (configured once per project); the **CLI** with no configuration at all |
| Agent with no shell (chat app, hosted agent) | MCP client only | MCP over **Streamable HTTP** |
| CI and pre-commit | A shell | **CLI** (`check`, `index`) — the replacement for `docs_index.py --check` |
| A person, a web UI | Browser | The existing **assets API** and UI over HTTP |
| Another program | HTTP or Rust/Python | The assets API, `/q`, or the Rust/Python library directly |

No single interface serves all of them, but they need not be separate implementations: every one
of them ends in **the same `ns-mem` and `ns-search` commands** evaluated in a Liquers
`Environment`. An interface is an adapter from its protocol to a query.

## 2. Options

| Option | For | Against |
|---|---|---|
| **CLI only** | Simplest; works for any agent with a shell; no process to manage; replaces the scripts directly | Every call re-opens the stores and rebuilds the environment; agents without a shell excluded; the output format must be learned from a skill |
| **HTTP server only** (assets API + `/q`) | Already exists; shared cache; many clients | An agent must be taught to build Liquers queries and URLs (escaping, `SEARCH_GUIDE.md` §4) — the tool surface is the whole query language; nothing tells it which calls exist |
| **MCP only** | The interface agents are built to discover: typed tools with schemas and descriptions; resources; prompts | Needs a client; MCP-specific code; for CI it is the wrong shape |
| **One binary, three faces** (recommended) | One library core, one configuration; `mcp` (stdio), `serve` (HTTP: assets API + MCP at `/mcp`), and one-shot subcommands | Three adapters to test — mitigated by all three calling the same command set |

**Recommendation:** a binary `liquers-agent` with subcommands, the MCP stdio face first and `serve`
after the MVP.

```
liquers-agent mcp      --config mem.yaml          # MCP over stdio — the MVP's primary face
liquers-agent serve    --config mem.yaml --port N # assets API + MCP over Streamable HTTP at /mcp
liquers-agent check    [PATH]                     # violations as JSON/YAML on stdout; exit 1 on errors
liquers-agent index                               # (re)materialize derived indexes (index.csv, …)
liquers-agent search   TEXT [--in PATH] [--limit N]
liquers-agent read     KEY [--tier title|description|full] [--section HEADING]
liquers-agent new      COLLECTION TITLE [--field k=v …] [--force]
```

The CLI's one-shot subcommands are the MCP tools under another name, so a skill can document both
in one table. Output goes to stdout as JSON or YAML (`CLAUDE.md` §"Diagnostic Output"); diagnostics
to stderr.

**Embedded, not client-server, by default.** `mcp` and the one-shot subcommands open the stores
in-process, as `docs_index.py` does. Derived assets (indexes, catalogues) persist in the store, so a
fresh process reuses them and recomputes only what a version change invalidated. `serve` is for
sharing one cache among several agents, and for agents with no shell.

## 3. Is MCP needed?

Yes, for three reasons the CLI cannot meet:

1. **Discovery.** MCP tools carry a name, a description and a JSON Schema for their arguments. An
   agent learns the memory's interface from the server, not from a skill it must first find.
2. **Structured results.** Tool results can be structured content (records) with text alongside,
   so tiered results arrive as data rather than as text to parse.
3. **Reach.** Agents without a shell, and hosted agents, speak MCP and nothing else.

It is not *sufficient*: an agent must still be told when to use memory and what the conventions are.
That remains a skill (`.claude/skills/liquers-memory/`, Phase 1 Documentation intent) — and the MCP
server can serve the skill text itself as a prompt (§4.3).

## 4. The memory MCP server

### 4.1 Tools

Ten tools in the MVP, each a thin wrapper over one `ns-mem` or `ns-search` query (`memory_context`
added in Phase 1 revision 3, from `brainstorm-workflow-improvements.md` §3 W4). Names are
prefixed `memory_` so that they cannot collide with a general Liquers MCP (§6).

| Tool | Annotations | Arguments | Returns | Capability |
|---|---|---|---|---|
| `memory_overview` | read-only | — | README L0/L1; collections with L0/L1 and counts; last log entries | R1 |
| `memory_list` | read-only | `path`, `depth` (1), `include_superseded` (false), `limit` | entries: `key`, `title`, `description`, `kind`, `status`, `is_dir`; `total` | R2, R6 |
| `memory_search` | read-only | `text` (search expression), `path`, `unit` (`document`\|`section`), `filter` (field expression), `limit` (10), `include_superseded` | hits: `key`, `title`, `description`, `excerpt`, `score`, `matched`, `line`/`heading` for sections; `shown` and `total` | R3, R4, R6 |
| `memory_read` | read-only | `key`, `tier` (`title`\|`description`\|`full`), `section`, `lines` | content, `version`, front-matter | R5 |
| `memory_describe` | read-only | `collection` (optional) | collections, fields, vocabularies, templates, writable mounts | R10 |
| `memory_new` | write | `collection`, `title`, `description`, `fields`, `body`, `force` | **either** duplicate candidates (nothing written) **or** the created key and version | W4, W1 |
| `memory_write` | write | `key`, `body`, `title`, `description`, `fields`, `expected_version`, `supersedes`, `reason` | key, new version | W1, W3, W5, W7 |
| `memory_edit` | write | `key`, one of: `replace {old, new}` \| `set_field {field, value}` \| `append_row {section, cells}`; `expected_version`, `reason` | key, new version | W2, W3, W7 |
| `memory_check` | read-only | `path` | violations: `rule`, `key`, `severity`, `message`; counts | M1, M5 |
| `memory_context` | read-only | `key` \| `topic` \| `files`, `budget` (tokens) | one Markdown context pack: L1 of the subject, matching reference sections, open issues and designs, code items; an "omitted" tail | AC-21 |

`memory_forget` (W6) is not one of the ten: it is NEXT, added when the archive collection
convention is fixed (Phase 1 decision 11); until then forgetting is `memory_edit … set_field
superseded_by` or a status.

Design rules for the tool set, from the survey and
`../store-and-asset-search/research-questions.md` §6:

- **Every hit is addressable.** `key` is a Liquers key, readable with `memory_read` and citable in
  another memory.
- **Every truncation is visible.** `shown`/`total` on every listing and search.
- **Every hit has a reason.** `matched` and `excerpt`.
- **Writes are explained.** `reason` goes into the write log (W7); the tool returns the versions.
- **No raw query tool in the memory MCP's MVP.** A `memory_query` that evaluates an arbitrary Liquers
  query is the escape hatch an expert agent wants, and the injection surface a cautious deployment
  does not. It belongs to the general MCP (§6), behind `COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`.

### 4.2 Resources

Each memory document is an MCP **resource** with URI `liquers://mem/<key>` and MIME type from its
metadata; a **resource template** `liquers://mem/{+key}` makes the whole memory addressable, and
the resource listing carries `title` and `description` — the L0/L1 tiers map onto MCP's own resource
fields. Clients that let a user attach resources (`@` mentions in Claude Code) thereby get the
memory with no tool call. Subscriptions (`resources/subscribe`) are NEXT: they would ride the asset
layer's change notifications.

### 4.3 Prompts

MCP prompts are named, parameterized message templates the client can offer to the user. Two
natural sources, both NEXT unless Phase 2 finds them cheap:

- **Procedural memories as prompts.** A `skills` collection's documents (front-matter `name`,
  `description`, body) map one-to-one onto prompts. A skill stored in memory is then usable by any
  MCP client, not only by hosts that read `.claude/skills/`.
- **Workflow prompts** from `_templates/`: "file an issue", "record a finding", "consolidate
  `inbox/`" — each a short procedure ending in the tool calls above.

### 4.4 Server instructions

The MCP `initialize` result carries free-text instructions. The memory server returns the memory
README's L1 plus the one rule every memory system's documentation repeats: *read the overview
before acting; search before writing; write L0/L1 with every memory.*

## 5. Model help without model credentials

MCP *sampling* lets a server ask the client to run a completion with the client's model, subject to
the client's approval. It is the route for W10 (summaries, extraction) that keeps P2 — the service
holds no credentials and the user sees each request. Client support varies, so it is NEXT and every
tool must work without it.

## 6. A general Liquers MCP, and combining the two

A general Liquers MCP — evaluate a query, validate a query (`liquers-validate`), discover commands
(`ns-search/commands`), browse stores and assets — is a different product: its user is an agent
*building* with Liquers, not one *remembering* with it. It is not designed here, but three
decisions keep the two combinable:

1. **A crate `liquers-mcp` holds the MCP plumbing**, not `liquers-axum` (MCP over stdio is not HTTP)
   and not `liquers-lib` (an engine that serves any command set should not depend on one). It
   depends on `liquers-core` for queries and command metadata, and on an MCP SDK — `rmcp`, the official Rust SDK, which provides stdio and
   Streamable HTTP server transports; its version and MSRV are checked in Phase 2.
2. **A tool is a declared query.** Every tool is `(name, description, argument schema, query
   template, result projection)`. For a tool that wraps one command, the argument schema is
   generated from its `CommandMetadata` — names, types, defaults, enum values and per-argument
   descriptions (`ArgumentInfo::description`, added 2026-10-11 by `ARGUMENT-INFO-HAS-NO-DESCRIPTION`)
   — so it cannot drift from the command. What the metadata cannot supply comes from the declaration:
   tool-level arguments of composite tools (`memory_new` is `similar` then `new`; `memory_edit` chooses among
   `edit`, `set_field` and `append_row`; `include_superseded` adds a filter). A write tool's `body`
   travels as the command's payload, not as a query parameter. Queries are built with
   `ActionRequest`, never by string concatenation (`SEARCH_GUIDE.md` §6). A *tool
   set* is a list of such declarations, data in the configuration.
3. **A server hosts several tool sets**, each with a name prefix (`memory_`, `liquers_`). Combining
   the memory MCP and a general one is listing two tool sets in one configuration; a name collision
   is a configuration error at start-up.

The memory MCP in the MVP is therefore one tool set, declared in data, served by a generic engine.
The engine is the MVP's cost; the general MCP later costs only its tool-set declaration and the
restricted-context work it needs.

## 7. Transport order

| Step | Face | Why this order |
|---|---|---|
| 1 | Library: `ns-mem` commands + `liquers-mcp` engine | Everything else is an adapter over it |
| 2 | `liquers-agent mcp` (stdio) | Serves the primary user — a coding agent in this repository — with no server to run |
| 3 | `liquers-agent check` / `index` / `search` / `read` / `new` | CI and shell-only agents; the oracle comparison against `docs_index.py` (AC-9, AC-10) runs here |
| 4 | `liquers-agent serve` (assets API + `/mcp`) — **NEXT** (Phase 1 question 5) | Sharing and agents without a shell; the assets API exists already, so the new part is mounting the MCP service in the router |
