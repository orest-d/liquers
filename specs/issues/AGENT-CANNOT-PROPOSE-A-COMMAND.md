---
id: AGENT-CANNOT-PROPOSE-A-COMMAND
kind: feature
title: An agent has no sanctioned way to propose a new command into a staging area
status: draft
priority: P3
complexity: L
area: [mcp, core/commands, py, docs]
design: agent-command-proposal
created: 2026-10-21
github:
---

## Problem

When the command registry lacks a verb an agent needs, the agent connected to `liquers-agent`
cannot add one. `liquers-mcp` serves only commands already registered when the server starts
(`ToolRegistry::build` in `liquers-mcp/src/server.rs` resolves every tool against the
`CommandMetadataRegistry` once), and `liquers-agent` has no tool that registers anything. The agent
falls back to code outside Liquers, whose results are uncached and carry no provenance.

This is G3 of `AGENT-WORKBENCH-SURFACE`, split out on 2026-10-21 because it is a project of its own:
it needs a staging place (namespace, realm or registry overlay), provenance in the proposed
command's metadata, and a promotion step a person performs.

## Problem Example

An agent needs `normalize_region` (map `"EU-West"`, `"eu"`, `"Europe"` to `EU`) before
`ns-rec/eq-region-EU`. It can express it as an alias of an existing command with fixed arguments, or
as a Python function, but there is no tool to submit either. Wanted: a call such as
`liquers_propose(kind=alias, name=normalize_region, …)` that registers the command in a staging
realm that is disabled unless the profile enables it, stamps proposer, session and date in its
metadata, and lists it for a person to promote.

`normalize_region` itself is not expressible as an alias at HEAD (04667a8): no registered command
recodes values, so it needs the Python slice. An alias that is expressible today, and which the
design uses as its example: `drafts`, an alias of `rec/eq` with heads `status`, `draft`, so that
`-R-key/./-/ns-mem/list-0-false-1000/-staging/drafts/drafts.csv` returns the rows of
`…/ns-rec/eq-status-draft/drafts.csv` (both validated with `liquers-validate`, the first with the
proposal as a `--registry-file` overlay).

## Impact

The workbench's "agents coin verbs, people govern the vocabulary" step
(`design/agent-memory-mvp/workbench-direction.md` W3) has no starting point. Workaround: a person
writes the command in Rust or Python and restarts the server.

## Expected behaviour

A first slice without Python: proposals as **alias commands** (`COMMAND_ALIASES.md`; a declaration
with `definition: Alias`) registered at run time into a staging realm, with provenance fields, and
a listing of proposals. Python-function proposals follow once `liquers-agent` can host `liquers-py`
(an embedded interpreter, a separate decision). Promotion stays a human act (move the declaration
into the governed set, regenerate `specs/command_registry.yaml`). Depends on post-start
registration (`design/post-init-command-registration/`) and on realms (`REALMS.md`).

## Discovery

Split from `AGENT-WORKBENCH-SURFACE` during the bulk design pass of 2026-10-20/21 (P-agent project
plan), when that issue was re-checked at commit 18732dd and re-scoped to G1, G2 and G4.
