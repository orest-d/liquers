---
id: ASSETS-API-ADMIN-OPERATIONS
kind: feature
title: Several client-meaningful AssetManager operations have no HTTP endpoint
status: draft
priority: P3
complexity: M
area: [axum, core/assets]
design: 
created: 2026-09-27
github:
---
## Problem

`specs/design/axum-assets-endpoints/` inventoried every `AssetManager` method and exposes only the
documented assets endpoints plus what the agent memory MVP requires or can use. These
client-meaningful operations were deliberately left without an endpoint:

| Operation | Endpoint it would get |
|---|---|
| `contains` | `GET contains` — largely redundant with `GET info`'s 404 |
| `version` | `GET version` |
| `get_binary_any_status` — recovery read of an `Expired` (or otherwise non-readable) value | `GET recover`, entry format (a separate route was preferred over a query parameter) |
| `to_override` — pin the current value | `POST override` |
| `makedir` | `PUT makedir` |
| `trigger_dependency_audit` / `trigger_dependency_audit_all_registered` | `POST audit/{*query}` / `POST audit` |
| `refresh_command_versions_and_expire` | `POST refresh_command_versions` |
| `eval_mode`, `is_started` | `GET manager` |
| a guarded `remove` refusing `Source` / `Override` (for cache-clearing clients) | `POST remove_cached` |

## Impact

Low. Everything except recovery reads and overrides can be reached in-process; operators and
tools that want them over HTTP have no route. The audit and refresh operations change shared
state and would need the access switches (or `CORE-SESSION-AND-KEY-ACL`) before being exposed.

## Expected behaviour

Add endpoints as concrete use cases appear, each following the conventions the design above sets:
the §3 envelope, key-only for mutations, refusals as `NotSupported` or 409, and mutations behind
`AssetsApiBuilder::read_only()`.

## Discovery

Scoping decision in Phase 1 of `specs/design/axum-assets-endpoints/`, 2026-09-27: the user asked
that new endpoints be justified by the agent memory MVP.
