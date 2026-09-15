---
id: AGENT-MEMORY-MVP
kind: design
title: MVP for an agent memory service on the liquers stack
status: draft
phase: high-level
area: [axum, lib/commands, core/store, docs]
issues: [AGENT-MEMORY-SERVICE]
created: 2026-09-15
---
# Agent memory service

## Question

What would it take to turn Liquers into a memory system for coding agents — comparable in role to
[OpenViking](https://github.com/volcengine/OpenViking) — using `liquers-axum` as the interface,
stores for storage, and the command system as the tool layer? At a minimum it should manage this
project's own issues, design lifecycle, skills and documentation.

## Answer, in one paragraph

Less than it looks, because the hard half is already built. A store is the filesystem an agent
memory needs; a `Key` is the path; a `Query` is a path *plus a derivation*; a recipe declares a
derived key; and the asset layer caches that derivation with a content-hash version and a
dependency record, so it recomputes when its source changes. That last property — lazy, cached,
dependency-invalidated derivation — is exactly what a tiered memory (abstract / overview / full)
needs, and it is the part a memory system normally has to build from scratch. What Liquers does
not have is a corpus-facing command namespace, content search, a service binary, and a tool
surface an agent can call. That is the MVP.

## Documents

- [`phase1-high-level-design.md`](phase1-high-level-design.md) — the analysis, the mapping onto
  existing parts, the gaps, and the proposed MVP with its milestones and acceptance test.

## Status

Phase 1 only, written as an analysis in response to a brainstorming question. Nothing here is
approved and nothing is implemented. The gaps it identified are filed separately:
`CORE-METADATA-NO-APPLICATION-ATTRIBUTES`, `STORE-NO-CONTENT-OR-METADATA-SEARCH` and
`STORE-WRITE-HAS-NO-PRECONDITION`. The MVP is designed to need none of them fixed first.
