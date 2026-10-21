---
id: MCP-SERVER-APPLIES-NO-ACCESS-POLICY
kind: feature
title: The MCP server limits its query tool by a command allow-list, not by an access policy
status: draft
priority: P3
complexity: M
area: [mcp]
design:
created: 2026-10-20
github:
---

## Problem

`liquers-mcp/src/server.rs` `check_allowed` walks a plan and refuses commands outside a per-tool-set
allow-list; resource and asset steps are always allowed, so a query tool can read any key the store
serves. `specs/design/agent-memory-mvp/phase2-architecture.md` (Q on the allow-list) records that
the allow-list "should give way to permissions checked by core" once access control exists.

## Impact

Low while the agent memory is single-user. A shared or hosted MCP server cannot restrict which keys
an agent reads or writes.

## Expected behaviour

Once `specs/design/access-policy-at-entry-points/` lands, the MCP server takes an
`Arc<dyn AccessPolicy>` and a configured principal (per profile or per tool set) and checks
`plan_access_requests` instead of, or in addition to, the allow-list. Whether the allow-list stays as
a tool-set restriction or becomes policy rules is the design's choice.

## Discovery

Split out of `access-policy-at-entry-points` (bulk design, 2026-10-20) as a follow-up slice, so the
first slice stays core + `liquers-axum`.
