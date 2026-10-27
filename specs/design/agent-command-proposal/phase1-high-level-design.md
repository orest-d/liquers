# Phase 1: High-Level Design — agent-command-proposal

## Purpose

An agent connected to `liquers-agent` cannot add a verb the registry lacks, so it computes outside
Liquers, with no cache and no provenance. This design gives it a sanctioned first step. The agent
proposes an **alias** of a command it may already run, with some arguments fixed. The alias is
registered at run time in a **staging realm**, carries provenance, and is persisted for a person to
review and promote. This is W3 of `design/agent-memory-mvp/workbench-direction.md`: "agents coin
verbs; people govern the vocabulary" (`reference/VISION.md` §The agent workbench). First slice: no
Python.

## Problem Example

Verified at HEAD (04667a8) with the `liquers-agent` test fixture memory. An agent repeatedly lists
the draft issues:

```
memory_query  -R-key/./-/ns-mem/list-0-false-1000/ns-rec/eq-status-draft/drafts.csv
```

It wants a verb `drafts` for "rows whose status is draft". Today there is no tool for that.
`ToolRegistry::build` (`liquers-mcp/src/server.rs`) fixes the tools at start, and nothing registers
a command after the environment is shared. Wanted:

```
memory_propose  {"name": "drafts", "target": {"namespace": "rec", "name": "eq"},
                 "head": ["status", "draft"], "doc": "Rows whose status is draft"}
memory_query    -R-key/./-/ns-mem/list-0-false-1000/-staging/drafts/drafts.csv
```

The proposal returns `staging--drafts`. The second call returns the same rows as the first, with no
restart. `_proposals/drafts.json` records the proposal with its proposer, session and date, and a
person decides whether `drafts` joins the governed registry. Both queries validate with
`liquers-validate`; the first needs the proposal file as a `--registry-file` overlay, and its plan
is `staging--drafts`, an alias of `-rec-eq`.

## Scope and Acceptance Criteria

- **AC-1** Propose and use
  WHEN the propose tool stages `drafts` (target `rec/eq`, heads `status`, `draft`)
  THEN it returns `staging--drafts`, and the query tool evaluates `…/-staging/drafts/drafts.csv` to
  the same rows as `…/ns-rec/eq-status-draft/drafts.csv`, without a restart
- **AC-2** Provenance
  WHEN a proposal is staged
  THEN its command metadata carries the hint `proposal` with `proposed_by`, `session` and
  `proposed_on`
- **AC-3** Listing
  WHEN the list tool is called
  THEN it returns every staged command with its target, heads, doc, provenance and file, and no
  command outside the staging realm
- **AC-4** Persistence
  WHEN the server restarts with the same profile
  THEN every proposal file is registered again, and a file that no longer validates is skipped
  with a warning naming it while the others load
- **AC-5** Off unless enabled
  WHEN the profile has no `proposals` section
  THEN neither tool exists, the folder is not read, and `-staging/drafts` is not registered
- **AC-6** No escalation
  WHEN the target is outside the set's query-tool allow-list, is an alias, or is in a
  non-executable realm
  THEN the proposal is refused, and nothing is registered or written
- **AC-7** Alias rules apply
  WHEN the heads and arguments do not line up with the target (`COMMAND_ALIASES.md` §2-3), or the
  name is invalid or reserved
  THEN the proposal is refused with the alias error, and nothing is registered or written
- **AC-8** Re-proposal
  WHEN a staged name is proposed again
  THEN it is refused without `replace: true`; with it, the alias is replaced and the next evaluation
  through it uses the new definition
- **AC-9** Reviewable file
  WHEN a person passes a proposal file to `liquers-validate --registry-file`
  THEN queries using the staged command validate, so the person can review it before promoting it
- **AC-10** Declaration checked at start
  WHEN a tool set declares `proposals` without a query tool, or with an empty, malformed or
  non-executable realm
  THEN building the tool registry fails with a configuration error naming the set

Non-goals: Python-function proposals (they need `liquers-agent` to host `liquers-py`); automatic
promotion of any kind; withdrawing or editing a proposal through a tool (a person deletes the file);
link heads; multi-user identity (`CORE-SESSION-AND-KEY-ACL`).

## Core Interactions

Commands (aliases registered after sharing, in a realm of their own); MCP tool sets (two tool kinds);
Store (one JSON file per proposal); Query (a `-staging/` segment header). No new value type.

## Crate Placement

`liquers-mcp` holds the tool kinds, because the engine is generic: any binary may offer proposals.
`liquers-agent` adds the profile setting and calls the restore at start. Nothing is added to
`liquers-core` beyond what `post-init-command-registration` provides.

## Documentation Intent

- Reference: extend `specs/reference/AGENT_MEMORY_SERVICE.md` §9 (the tool kinds and the profile),
  `specs/reference/COMMAND_ALIASES.md` (runtime aliases, the `proposal` hint) and
  `specs/reference/REALMS.md` (a staging realm).
- Guide: extend `specs/guides/COMMAND_REGISTRATION_GUIDE.md` with "Promoting a proposed alias".
- Other documents: none.
- Documents to update: the source issue (closed by the implementer).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — size `L` (rule 1); new tool kinds, `pub` types and a
  configuration schema change (rules 3 and 4).
- **Leading issue:** **Open design - depends on an undecided core contract.** Run-time registration
  is `post-init-command-registration`'s `EnvRef::update_commands`. That design is `needs-decision`
  on its trait shape. This design assumes its recommended answer (a), the snapshot handle, but it
  uses only `update_commands`, which exists under (a) and (b) alike.
- **Explanation:** a working solution is fully specified on top of that API. The open items are
  where proposals live and what they may wrap, and each has a recommended answer.
- **Open questions:**
  1. **Open design - run-time registration** (above). Under (c) *not planned* this design is
     blocked: an `EnvRef` cannot gain an alias.
  2. **Open design - staging realm or namespace.** Recommended: a realm (`staging`, configurable).
     The `-staging/` header makes a staged verb visible in every query that uses it, and it cannot
     shadow a governed command. Promotion changes the key, so results are recomputed once.
     Alternative: a namespace `ns-staging` in the default realm. It is shorter to write, but a
     staged `eq` and `rec/eq` would then be told apart only by the order of the segment's `ns-`
     instructions.
  3. **Proposed resolution - what may be wrapped.** The target must match the same set's query-tool
     allow-list, be `Registered` and be executable; heads are always literal values (a link head is
     a non-goal). A staged alias then
     runs through the allow-list check under its target's key (the plan step is the target), so it
     can do nothing the agent could not already do.
  4. **Proposed resolution - provenance.** A command-level hint `proposal: {proposed_by, session,
     proposed_on}`. `proposed_by` is the profile's `proposer`, `session` is the server's start time
     and `proposed_on` the proposal's time. A new `CommandMetadata` field was rejected: it would
     change the registry format for one producer. The MCP client's own name is a follow-up.
  5. **Proposed resolution - persistence.** One JSON file per proposal under the configured
     `folder`, shaped as a `CommandMetadataRegistry` with one command, so it doubles as a
     `liquers-validate` overlay (AC-9). JSON needs no new dependency in `liquers-mcp`.

## Design Dependencies

- `post-init-command-registration` — **requires**: `EnvRef::update_commands` (its Phase 2), which
  also refreshes metadata versions and expires the dependents of a replaced alias (AC-8). Its
  `needs-decision` is carried here as question 1.
- `AGENT-WORKBENCH-SURFACE` (feature, no design) — **overlaps**: both add tool kinds to
  `ToolSetDecl` / `ToolEntry` (`liquers-mcp`). The additions are independent fields and variants,
  so either order works with a mechanical rebase. G3 was split out of it on 2026-10-21 on purpose.
- `command-alias-contract` (complete) — the alias rules this design relies on; not extended.
- `multi-realm-dispatch` — no dependency: a staging realm runs in the one interpreter.

## Consolidated Findings

- Verified with `liquers-validate` and an overlay: an alias in realm `staging` targeting `rec/eq`
  plans to one `Action` for `-rec-eq` with `origin: Alias(staging--drafts)`. `check_allowed`
  therefore sees the target, so no allow-list change is needed and a staged alias cannot escalate.
- `McpServer` is generic over `E: Environment`, and `CommandExecutor` has no registration method.
  Registration therefore sits behind `McpServer::with_proposals`, bounded by
  `E: Environment<CommandExecutor = CommandRegistry<E>>`. The `ServerHandler` impl stays generic.
- `ToolRegistry` captures command metadata at build time for the `commands` resource, so staged
  commands appear only in the list tool. That is acceptable and documented.
- The proposal is validated by `register_alias` inside the `update_commands` closure, together
  with `CommandMetadata::check` for reserved names. On `Err` nothing is published. The file is
  written after a successful registration, and a failed write unregisters the alias again.
- Implementation must wait for `post-init-command-registration` Phase 4. Before that, only
  Steps 1-2 (the declarations and the validation helpers) can land.
