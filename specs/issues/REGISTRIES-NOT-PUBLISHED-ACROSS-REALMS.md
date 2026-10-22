---
id: REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS
kind: feature
title: A server does not publish its command and type registries, so a client cannot plan for the server's realm
status: draft
priority: P3
complexity: L
area: [axum, web, core/commands, core/value]
design:
created: 2026-10-20
github:
---

## Problem

A client environment (typically `liquers-web`) that wants to plan a query step for a server's realm
needs the server's `CommandMetadataRegistry` and, once `design/realm-type-registry/` lands, its
`TypeRegistry`. Nothing serves either: `liquers-axum` has Store, Assets, Recipes and Query routes
(`WEB_API_SPECIFICATION.md` §1) and none returns a registry. A page can import command metadata it
already has (`registerCommands` with a `command_registry.yaml` string,
`liquers-web/src/command/import.rs`), but has no route to fetch it from the server it talks to, and
importing it today places the commands in whatever realm the file says (normally `""`), not under
the name the page uses for the server.

## Impact

The client/server shape of `VISION.md` cannot plan a server-realm step without hand-copied
registry files that drift from the server. Workaround: export the registry at build time
(`export-command-registry`) and ship it with the page.

## Expected behaviour

- A read-only route (its own builder, off by default) returning the server's executable command
  metadata (`CommandMetadataRegistry::executable_commands()`) and its type registry as a list of
  `TypeInfo`.
- A client import that places both under a chosen realm name (`TypeRegistry::import_realm` from
  `realm-type-registry`; the command equivalent is new), so `-server/…` plans against them.
- Freshness: how a client learns the registry changed (a version or ETag on the route).

## Discovery

Bulk design 2026-10-20 (cluster C6), split off from `TYPE-REGISTRY-NOT-REALM-AWARE` behaviour 2 and
`CORE-MULTI-REALM-INTERPRETER` (`design/multi-realm-dispatch/` Phase 1 question 4). Requires
`design/realm-type-registry/`; required by `design/multi-realm-dispatch/`.
