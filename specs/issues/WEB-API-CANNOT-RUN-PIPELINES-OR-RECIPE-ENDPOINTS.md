---
id: WEB-API-CANNOT-RUN-PIPELINES-OR-RECIPE-ENDPOINTS
kind: feature
title: The web API cannot run a pipeline or recipe document, nor serve recipes as typed endpoints
status: draft
priority: P2
complexity: L
area: [axum, core/assets, core/context, mcp]
design:
created: 2026-11-01
github:
---

## Problem

`specs/design/composite-command-definition/` adds pipelines: a structured, serializable form of a
query whose arguments can be values, placeholders, links or nested pipelines, usable in composite
commands and recipes. The web API (`specs/reference/WEB_API_SPECIFICATION.md`) cannot use them:

- a request can send only a query path plus overrides of its **last** action (§2.4), never a
  pipeline or a whole recipe document;
- a request cannot supply the value a pipeline starts from (a JSON document, or later an uploaded
  table) other than by first writing it to the store;
- recipes cannot be offered as **typed endpoints**: there is no listing of recipes with declared
  parameters and argument schemas, and no route that calls one with a JSON object of arguments,
  which is what an agent needs as a tool catalogue;
- a pipeline cannot read **payload** values (for example the authenticated principal), because
  `PayloadType` (`liquers-core/src/commands.rs`) exposes no fields.

## Impact

Agents and web clients must assemble query text, and cannot pass structured data to any action but
the last. Endpoints for agents need a Rust command each. Workaround: register a composite command
and call it by query, with overrides for its arguments.

## Expected behaviour

Analysed in `specs/design/composite-command-definition/notation-and-integration.md` §5 and §7,
moved out of that design by the maintainer on 2026-11-01:

1. `POST q/recipe/data|entry|submit` and WebSocket `subscribe` with `recipe` accept a recipe
   document (`query` and/or `pipeline`, `arguments`, `links`). Such a request is unmanaged unless
   it has a text form.
2. Endpoint recipes: `GET recipes/endpoints` lists recipes that declare `parameters`, each with a
   JSON Schema of its arguments, and `POST recipes/call/{*key}` binds a JSON object to one.
   `liquers-mcp` can serve them as MCP tools.
3. A `payload` pipeline source, through a new `PayloadType` field accessor. It is read only from the
   server-side context and never bindable from a request; it is payload-required, so never cached
   and refused under a key. Gated on `design/access-policy-at-entry-points/`.
4. Later: a raw request body (`Content-Type: text/csv`) as the starting state.

Open, decided later by the maintainer (2026-11-01): whether recipes, or something similar, define
these endpoints, and whether they may also define commands. At HEAD of the composite design, recipes
define assets only, and a placeholder in a recipe pipeline is a planning error.

A design is needed (complexity `L`).

## Discovery

The maintainer's review of `composite-command-definition` on 2026-11-01, which split the web part
out of that design.
