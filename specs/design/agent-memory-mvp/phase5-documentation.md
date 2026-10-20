# Phase 5: Documentation - agent-memory-mvp

## Completion Preconditions

- [x] Implementation is finished and validated
- [x] All user comments are answered or incorporated (the AC-11 card: "Read fresh", 2026-10-19; Phase 5 approved 2026-10-20)
- [x] All review comments are answered or incorporated (PR #140: the description was rewritten to the real scope)
- [x] Documentation is consistent with implemented and tested behavior
- [x] Documentation is included in the implementation PR when practical (PR #140)

## Implementation Summary

The three layers of Phase 2 are built, in the order of Phase 4, one commit per step
([Progress](phase4-implementation.md#progress)):

- **`ns-mem`** (`liquers-lib/src/mem/`, feature `mem`, on by default): the schema
  (`_schema/memory.yaml`, versioned), a line-preserving front-matter editor, the L0/L1 bridge and
  the `markdown_tiers` store handler, one write path (precondition, validation, stamps,
  `set_binary`, log line), and the ten commands. `check` implements the schema rules and seven
  named rules ported from `docs_index.py --check`.
- **`liquers-mcp`**: tool sets of single-command queries with generated input schemas, a
  structurally filled key argument, hidden relative defaults, the allow-listed query tool, and
  tool, text and command-reference resources.
- **`liquers-agent`**: one YAML configuration (the environment plus `memory`), the built-in
  `memory_` tool set and query guide, and the `mcp`, `check`, `search`, `read` and `query`
  subcommands over the same server.

It conforms to the approved design, with the deviations listed in
[Phase 4](phase4-implementation.md#deviations-from-the-plan). One changed an approved contract and
went to Orest: with a document already loaded, a hand edit was not seen (AC-11). He chose "Read
fresh" (2026-10-19): `-mem-read` also takes a key, reads it from the store, and is volatile, and
`memory_read` uses that form.

Added beyond the plan: the schema's `instructions` are shown by `overview`, so they reach the MCP
instructions (the field had no reader). Omitted: nothing in scope; the generated indexes, templates
and context packs belong to `agent-memory-derived`.

## Documentation Delivered

### New Reference Documents

- [`reference/AGENT_MEMORY_SERVICE.md`](../../reference/AGENT_MEMORY_SERVICE.md): layout, the
  schema format, tiers, the commands, the write path and log, `check`, storage, MCP tool sets, the
  CLI and the two profiles.

### New Guide Documents

- [`guides/AGENT_MEMORY_MIGRATION_GUIDE.md`](../../guides/AGENT_MEMORY_MIGRATION_GUIDE.md): what
  is durable and what is derived, the rules that keep it so, and what to do per kind of change;
  from [`migration-guide-draft.md`](migration-guide-draft.md), checked against the code. It adds
  rule 6 (no recipe outputs in `storage: none` mounts).
- `.claude/skills/liquers-memory/SKILL.md`: how an agent uses the memory tools.

### Existing Documents Reviewed or Updated

| Document | Change |
|---|---|
| `reference/PROJECT_OVERVIEW.md` | `liquers-mcp`, `liquers-agent` and the `mem` feature in the crate structure and dependency flow |
| `guides/STORE_CONFIG_GUIDE.md` | `markdown_tiers` in the handler catalogue; `storage: none` for hand-edited checkouts |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | §5 *Commands served as MCP tools* |
| `DOCS_STRUCTURE_GUIDE.md` | area `mcp`; filing an issue through `memory_new` (§4.8.1) |
| `CLAUDE.md` | the new crates, the `mem` feature, the test commands, registry tests gated on six groups |
| `scripts/docs_index.py`, `memory/_schema/areas.yaml` | area `mcp` |

Each reference and guide change carries a History row and a `reviewed:` bump.

### Links and Capability Map

`specs/README.md`: the agent memory line is now *built* and links the reference, with the guide
and this design in its note; its paragraph describes MCP as the client interface; a "When to use
what" row points to the reference.

## Issues Filed

- `RECIPE-RESULT-UNDER-STORAGE-NONE-READS-AS-SOURCE`: a computed result in a `storage: none` mount
  reads back as `Source`; migration guide rule 6 works around it.
- `SPECS-FRONT-MATTER-IS-NOT-VALID-YAML`: nine specs files whose front-matter only the lenient
  reader accepts.
- `LOG-ENTRY-HAS-NO-SOURCE`: engine diagnostics and command notes share one log, told apart only by
  text prefixes in `liquers-mcp`.
- `AGENT-MEMORY-HAS-NO-MIGRATE-COMMAND` (Phase 3): schema migrations are hand edits.

## Important Learning

- **A loaded asset is a cache with no invalidation from the store.** `version_verification` did
  not cover a hand edit in a running service; reading from the store when freshness matters is the
  rule, and it is why `read` takes a key.
- **The same command set serves three interfaces.** Liquers queries, MCP tools and the CLI share
  one implementation; the MCP layer is generic (it knows nothing of memory), so other command sets
  can be served the same way ([`COMMAND_REGISTRATION_GUIDE.md`](../../guides/COMMAND_REGISTRATION_GUIDE.md) §5).
- **An oracle test keeps a port honest.** `check_matches_docs_index_oracle` compares `check` with
  `docs_index.py --check` over the real specs and seeded violations; it found the lenient
  front-matter, link-scope and skills-path mismatches.
- `s3s-fs` serves OpenDAL's S3 client in process, so S3 mounts can be tested offline. It needs Rust
  1.96, one above the workspace minimum, for `liquers-agent`'s tests only.

## Conformance and Remaining Work

Requested, approved and implemented scope agree: AC-1 to AC-20 have tests (AC-21 and AC-22 moved to
`agent-memory-derived`). Remaining work is filed: the issues above, plus the derived layer in
`design/agent-memory-derived/`.

## Validation

- `cargo test -p liquers-lib --lib --tests`, `cargo test -p liquers-mcp`,
  `cargo test -p liquers-agent`: see Phase 4 Step 13.
- `bash scripts/check-build-matrix.sh`: all rows, including the new `mem`, `webui,mem` (wasm32),
  `liquers-mcp` and `liquers-agent` rows.
- `python3 scripts/docs_index.py` then `python3 scripts/docs_index.py --check`: 0 errors.
