---
title: Agent Memory Service
kind: reference
audience: internal
area: [lib/commands, mcp]
reviewed: 2026-10-27
---
# Agent Memory Service

A file-based memory for agents: Markdown documents with front-matter, in folders, read and written
through ten `ns-mem` commands and served over MCP and the command line by `liquers-agent`. The
documents stay ordinary files a person can edit; everything else (titles, tiers, listings,
versions) is derived from them. How to change a memory without losing anything is
[`guides/AGENT_MEMORY_MIGRATION_GUIDE.md`](../guides/AGENT_MEMORY_MIGRATION_GUIDE.md); why it is
built this way is [`design/agent-memory-mvp/`](../design/agent-memory-mvp/DESIGN.md).

## 1. Layers

| Layer | Where | What it does |
|---|---|---|
| `ns-mem` commands | `liquers-lib/src/mem/`, feature `mem` (default; pulls `records`) | Read the memory's schema, list, search, read, create, write, edit and check documents. Usable from any Liquers query |
| MCP engine | `liquers-mcp` (depends on `liquers-core` and `rmcp` only) | Serves any declared set of single-command queries as MCP tools, with schemas generated from `CommandMetadata`, plus an allow-listed query tool and resources |
| Agent binary | `liquers-agent` | Reads one configuration, builds the environment with every library command and the store factory (S3 and the other OpenDAL services), and serves the built-in `memory_` tool set over stdio or as CLI subcommands |

Each layer is usable without the next: a query such as
`-R/specs/issues/X.md/-/ns-mem/read-l1` works in any Liquers environment with `mem` on.

## 2. A memory on disk

A memory has a **root** key. Under it:

| Path | Content |
|---|---|
| `_schema/memory.yaml` | The schema (§3). Read through the store on every command call |
| `README.md` (the schema's `readme`) | The memory's own L0/L1, shown first by `overview` |
| `_log/YYYY-MM.md` (the schema's `log`) | One line per write, append-only (§6) |
| `_templates/…` | Templates a collection may name |
| collection folders | The documents |

The documents of a collection need not live in the root's own folder: the store's mounts decide
where each key is kept, so a collection can be a checkout folder, a read-only mount or an S3
bucket (§8).

## 3. The schema: `_schema/memory.yaml`

```yaml
version: 1                  # the schema format; a newer one is refused, naming both numbers
readme: README.md           # default
log: _log                   # default
instructions: Search before you write.   # shown by overview, so in the MCP instructions
tiers:                      # the L0/L1 bridge, memory-wide (defaults shown)
  l0: ["meta:title"]
  l1: ["meta:description"]
collections:
  - name: issues
    folder: specs/issues
    description: What is wrong or missing
    filename: "{id}.md"     # default
    id: { from: title, case: screaming_kebab, max_len: 60 }
    template: _templates/issue.md
    fields:
      id: { required: true, equals: filename_stem }
      status: { default: draft, enum_from: _schema/issue-status.yaml, transitions: _schema/issue-transitions.yaml }
      area: { list: true, enum_from: _schema/areas.yaml }
      design: { link: designs }
      created: { type: date }
    sections: [Problem]
    checks: [dead_links]
```

Collection settings:

| Setting | Meaning |
|---|---|
| `folder` | Relative to the root. A key belongs to the collection whose folder is its longest prefix |
| `file` | A single-document collection (`profile.md`) |
| `entry` | A folder is one memory, read through this file (`DESIGN.md`, `SKILL.md`) |
| `recursive` | Members are `.md` files at any depth, not only the folder's own |
| `writable` | `false` refuses every write; `describe` and `overview` report it |
| `id` | `from: title \| date \| date_title`, `case: screaming_kebab \| kebab \| as_is`, optional `max_len` |
| `fields` | In front-matter order. Each takes `required`, `default` (filled on create), `enum` or `enum_from` (a key of a YAML list), `list`, `equals: filename_stem`, `link` (target collection) with optional `reciprocal`, `transitions` (a key of a map from a value to its allowed successors) and `type: text \| date \| key` |
| `sections` | Headings every member must have |
| `history` | `{ section, stamp }`: appending a row to that section stamps that field with today's date |
| `checks` | Named built-in rules (§7) |
| `tiers` | Overrides the memory-wide bridge |

A tier source is written `kind:arg`: `field:<name>` (a front-matter field), `meta:<name>` (a field
of the store's metadata record), `section:<heading>` (its first paragraph), `heading` (the first
heading) or `paragraph` (the first paragraph after the front-matter). The first non-empty source
wins. L0/L1 serve agents finding a memory; `title`/`description` serve people. By default they are
the same, and changing which parts an agent sees is an edit of the schema, with no code change.

## 4. Commands

All ten are in namespace `mem`, async, and named here by `CommandKey` Display. Each ends with
`root: Key = query "-R-key/."`, the memory's root. The state is what the command acts on:

- **key state**: a `Value::Key`, from `-R-key/<path>`, so a folder is never evaluated and a
  document about to be replaced is not read as an asset;
- **document state** (`read` only, which also takes a key): the document's value, from `-R/<key>`.

Every command is `volatile: true`; no `ns-mem` result is ever written to a store.
Counts, truncation and versions go to the log as notes (`shown 10 of 47; raise limit to see more`,
`version 1a2b…`), so a table stays a table.

| Command | State | Arguments | Returns (default file) |
|---|---|---|---|
| `-mem-overview` | root | `log_lines = 10` | text: README L0/L1, the schema's `instructions`, collections with counts and writability, the latest log lines (`overview.md`) |
| `-mem-describe` | root | `collection = ""` | text: collections, fields, vocabularies, templates, history rules (`describe.md`) |
| `-mem-list` | folder | `depth = 1` (0 = unlimited), `include_superseded = false`, `limit = 100` | record view `key, l0, l1, kind, status, asset_status, is_dir` (`list.md`) |
| `-mem-search` | folder | `text`, `unit = document \| section`, `filter`, `limit = 10`, `include_superseded` | record view `key, l0, l1, excerpt, score, matched, heading, line` (`search.md`) |
| `-mem-read` | document key, or document | `tier = l0 \| l1 \| full`, `section`, `lines` | text; notes `key` and `version` (`read.md`) |
| `-mem-similar` | root | `collection`, `title`, `description`, `limit = 5` | record view of near-duplicates (`similar.md`) |
| `-mem-new` | root | `collection`, `title`, `description`, `fields`, `body`, `force`, `reason` | JSON `{written: false, candidates}` or `{written: true, key, version}` (`new.json`) |
| `-mem-write` | document key | `body`, `title`, `description`, `fields`, `expected_version`, `supersedes`, `reason` | JSON `{key, version_before, version_after}` (`write.json`) |
| `-mem-edit` | document key | pairs `old`/`new`, `field`/`value`, `section`/`row`; `expected_version`, `reason` | as `write` (`edit.json`) |
| `-mem-check` | folder | — | record view `rule, key, severity, message` (`check.md`); notes the counts and the `unsupported` rules |

Behaviour worth knowing:

- **Listing** goes through the asset manager, so a key that only has a recipe is listed with
  `asset_status: Recipe` and is not evaluated.
- **Search** joins the text, `filter` and the superseded test into one predicate before ranking, so
  the `shown of total` note never counts hidden memories.
- **`new`** runs `similar` first and returns the candidates (hits sharing at least half of the
  title's significant words) instead of writing, unless `force` is set. Otherwise it derives the
  id, takes `body` or the template's body as it is, applies field defaults (so `status: draft` is
  schema data, not code) and commits.
- **`edit`** applies every given pair to the text in memory, in the order replace, set field,
  append row, then validates and commits once. A half-given pair, or none, is refused before
  anything is read. `old` must match exactly once; `new` may be empty. `value` is a YAML scalar or
  flow list (`[a, b]`). Front-matter edits are line-preserving: every byte outside the changed
  line is kept, so a field change is a one-line diff. TOML front-matter is refused.
- **Supersession**: `write` with `supersedes` writes the new memory with `supersedes:` and then sets
  `superseded_by` on the old one, logging both. `list` and `search` hide a superseded memory unless
  `include_superseded` is set.
- **Versions** are `Version::from_content` of the bytes, 32 hex digits. `read` reports the same
  value `commit` compares, so `expected_version` from a read is what a write must pass. A mismatch
  is `Error::dependency_version_mismatch`.

## 5. The write path

Every write command uses one `commit`:

1. Read the current bytes from the store (absent means a new memory).
2. Compare their version with `expected_version`, when given.
3. Validate against the key's collection; refuse a key outside every collection or in one with
   `writable: false`.
4. Stamp `updated`, and `created` on a create (`YYYY-MM-DD`).
5. Write with `AssetManager::set_binary`, with a record carrying `title` and `description`.
6. Append the log line (§6).

If the data write succeeds and the log append fails, the data write stands; the error names the
key and says the log line is missing.

## 6. The log

One Markdown list item per write in `<root>/<log>/YYYY-MM.md`. Versions show their first 8 hex
digits; a create shows `∅`:

```
- 2026-10-17T15:20:58Z write specs/issues/X.md 1a2b3c4d→5e6f7a8b — reason text
```

The log is append-only. Readers skip lines they do not understand.

## 7. `check`

Schema rules: `required`, `enum` and `enum_from`, `list`, `id` (equals the filename stem),
`duplicate_id` (unique per collection), `link` and `reciprocal`, `sections`, `transitions` on the
current value, `supersede_reciprocal`, a title over 120 characters (warning), and `front_matter`
(a warning when the front-matter is not valid YAML; the fields are then read leniently as flat
`key: value` lines).

Named rules a collection lists in `checks`, ported from `scripts/docs_index.py --check`:

| Rule | Checks |
|---|---|
| `reviewed` | the field is present |
| `history` | a `## History` section with a row dated as `reviewed` |
| `dead_links` | relative links resolve, within the memory's folders |
| `lifecycle` | a design's status against its phases (reported as `status` and `phase`) |
| `readiness` | readiness and autofix labels against the design's sources |
| `compact_form` | a compact design has its `## Phase N` sections |
| `scenarios` | Phase 1 `AC-<n>` scenarios and the Phase 3 tests citing them |

Rules `check` does not implement are named in an `unsupported` note: stale `index.csv` and
`index.md` (generated files belong to `design/agent-memory-derived/`), dead links to source code,
and ids unique across collections. On this repository's profile, `check` reports the same errors
as `docs_index.py --check` (`liquers-agent/tests/specs_profile.rs`).

## 8. Storage

The memory needs no store of its own; a mount's `metadata` section
([`guides/STORE_CONFIG_GUIDE.md`](../guides/STORE_CONFIG_GUIDE.md)) does the projection:

- `on_read` with `front_matter` and `markdown_tiers` fills the record's `title` and `description`.
  `markdown_tiers(sections…)` fills an empty `title` from the first heading and an empty
  `description` from the first paragraph of the first listed section present, else the first
  paragraph. It never overwrites and never writes.
- `on_write` with `front_matter_sync` writes record changes back into the front-matter.
- `storage: none` keeps no sidecar: the record is re-derived on every read, so a hand edit is seen
  by the next read that loads the document. Use it for checkouts edited by hand.
- A recipe's result written into a `storage: none` mount (an `index.md` kept in git) has no stored
  record: the one read back is *inferred*, so the asset manager treats the file as having no
  metadata rather than as a user's `Source`. With the environment's default
  `assets: { external_change: user_input }` it is served as an `Override` after a restart (a hand
  edit of it is kept); with `external_change: corrupted` it is recomputed on the first read of each
  process, which keeps a generated file current at the cost of one evaluation. `remove` deletes it
  and the key drops back to its recipe. To keep records for results but none for user files, use a
  sparse mount instead: `storage: per_entry-root` with `infer-Source` at the end of `on_write`
  (`guides/STORE_CONFIG_GUIDE.md` §5).
- `read_only: true` on a mount, with `writable: false` on its collection, makes it reference
  material.

A running asset manager serves the copy it has loaded, and `list` and `search` load documents.
So `read` given a key (`-R-key/<key>`, as `memory_read` does) reads the bytes and record from the
store and sees a hand edit at once; given a document (`-R/<key>`) it reads the loaded copy.

## 9. MCP and the CLI

**Tool sets** (`liquers_mcp::ToolSetDecl`) declare tools as queries with exactly one action:

```yaml
prefix: memory_
cwd: ""                          # the memory root; set from `memory.root`
tools:
  - name: read
    query: "-R-key/./-/ns-mem/read/read.md"
    key: { name: key, required: true }
    read_only: true
query_tool:
  name: query
  allow: ["-mem-overview", "-mem-describe", "-mem-list", "-mem-search", "-mem-read",
          "-mem-similar", "-mem-check", "-rec-*", "-search-*"]
resources:
  - { kind: tool, uri_template: "liquers://memory/{+key}", tool: read, argument: key }
  - { kind: text, uri: "liquers://memory/query-guide", name: query-guide, description: "…", text: "…" }
  - { kind: commands, uri_template: "liquers://commands/{namespace}", namespaces: [mem, rec, search] }
```

- A tool's input schema is generated from the command's arguments (types, `enum`, defaults,
  labels as titles), plus the **key argument** and an optional `format`.
- The key argument is parsed as a `Key` and joined onto the query's first resource segment **in the
  parsed query**, never in its text. An absolute key, an empty, `.` or `..` element, or an element
  starting with `-` is refused, so a key cannot change the command or leave the root.
- `format` replaces the extension of the query's filename (`json`, `csv`, `md`, …).
- **Hidden arguments** — `injected` ones, `fixed` ones, and every argument whose default is a link
  with a relative operand, such as `root` — are not in the schema and are refused in a call.
- **`memory_query`** evaluates any query whose every action, including those of link parameters,
  matches the allow-list (an exact `CommandKey` Display, or a prefix ending in `*`). The allow-list
  is the safety boundary until a context can be restricted
  (`COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT`).
- A result is the serialized value as text; log notes travel as a second content block, so a CSV or
  JSON result stays parseable; a `json` result also carries structured content. An `Error` becomes
  a tool error with its message; only an unknown tool is a protocol error.
- The server's instructions are the tool set's `instructions`, the memory's `overview` (without log
  lines) and a pointer to the query guide.

**The built-in `memory_` set** (`liquers-agent/tools/memory.yaml`): `overview`, `describe`,
`list` (`path`), `search` (`path`), `read` (`key`), `new`, `write` (`key`), `edit` (`key`), `check`
(`path`) and `query`. The query guide (`liquers-agent/tools/query-guide.md`) is served as
`liquers://memory/query-guide`.

**`liquers-agent`** reads one YAML file: the Liquers environment configuration unchanged (`store`,
`recipes`, `assets`), plus

```yaml
memory:
  root: ""          # the memory's root key
  tools: [...]      # optional tool sets replacing the built-in one
```

| Subcommand | Calls |
|---|---|
| `mcp` | serves the tools on stdio |
| `check [PATH]` | `memory_check`; exits 1 when an error is found |
| `search TEXT [--in PATH] [--limit N]` | `memory_search` |
| `read KEY [--tier T] [--section S]` | `memory_read` |
| `query QUERY` | `memory_query`; prints the value in the format its filename selects |

`--config` (or `LIQUERS_AGENT_CONFIG`, default `liquers-agent.yaml`) names the file; `--format
json|yaml` the CLI output. Results go to stdout, notes and errors to stderr. Every subcommand calls
the same server as MCP does, through the same allow-list.

## 10. Profiles

**liquers-specs** (`memory/liquers-specs.yaml`, run from the repository root): this repository's
`specs/` as a memory. Collections `issues`, `designs`, `reference`, `guides`, `archive` (read
only), `skills` (`.claude/skills`, read only) and `notes`; the memory's own files are in
`memory/`. A filesystem mount's `path` is the folder its prefix lives in, so the `specs` mount's
path is `.`.

```bash
cargo run -p liquers-agent -- --config memory/liquers-specs.yaml check specs/issues
cargo run -p liquers-agent -- --config memory/liquers-specs.yaml mcp
```

**openviking**: category collections (`profile`, `preferences`, `entities`, `events` with a
required `date`) under `user/memories`, a `.overview.md` readme, and the `user` mount on S3
(`type: s3` with `bucket`, `root`, `endpoint`, `region` and credentials as `${ENV_VAR}`). It is
the same code with different configuration; `liquers-agent/tests/s3_profile.rs` runs it against
an in-process S3 endpoint and across a restart.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-27 | §8: recipe results in a `storage: none` mount are read through their inferred records (`external_change` decides), and the sparse `infer-Source` alternative. | `design/storage-none-computed-values/` |
| 2026-10-19 | Created. | `design/agent-memory-mvp/` Phase 5 |
