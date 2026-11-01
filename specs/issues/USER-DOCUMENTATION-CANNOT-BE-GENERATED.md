---
id: USER-DOCUMENTATION-CANNOT-BE-GENERATED
kind: feature
title: Nothing generates the user documentation site, and Liquers cannot build one from templates
status: accepted
priority: P2
complexity: L
area: [docs, build, lib/commands]
design: user-documentation-guides
created: 2026-11-01
github: 233
---
## Problem

No script builds the documentation. `cargo doc` is run only by hand and by the rustdoc checks of
individual issues; the Python and JavaScript API generators are not chosen yet
(`PY-API-REFERENCE-NOT-GENERATED`, `WEB-API-REFERENCE-NOT-GENERATED`). Nothing assembles user
guides, a homepage or indexes of the API docs around them.

Liquers has the pieces to do the assembly itself — Mustache templates with query tags
(`ns-template/render`, `expand`), stores over folders, the command registry and `commands_doc`
— but no tool runs them over a source folder and writes a site. Markdown-to-HTML conversion exists
only in the `webui` feature (`pulldown-cmark`, wasm).

## Impact

The documentation folder designed in `api-documentation-rules` would have no build, so its
homepage and indexes would be hand-maintained and drift. And the project misses the obvious
worked example of its own template system.

## Expected behaviour

Requested by the owner on 2026-11-01 (GitHub #233): a script generates the whole documentation —
the API docs with the usual tools (rustdoc, and the integrations' generators), then the user
guides and top-level pages produced with Liquers queries and Mustache, by a documentation tool
developed under its own design.

## Discovery

Requested by the owner (Orest) in the project thread on 2026-11-01, together with
`API-DOCS-HAVE-NO-CONTENT-RULES`.
