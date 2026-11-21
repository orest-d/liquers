---
id: NAMESPACES-HAVE-NO-METADATA
kind: feature
title: A namespace has no metadata, so a user-defined namespace cannot declare its icon, colour, label or doc
status: draft
priority: P3
complexity: M
area: [core/commands]
design: 
created: 2026-11-21
github:
---
## Problem

The command metadata registry (`liquers-core/src/command_metadata.rs`, `CommandMetadataRegistry`)
describes commands only. A namespace exists only as the `namespace` field of its commands and as
the `ns` lists of queries and pipelines (`liquers-core/src/pipeline/mod.rs` `Namespaces`); nothing
can say what a namespace is called, what it is for, or how a UI should show it.

## Impact

UIs cannot present namespaces well. The `query-editor-ergonomics` design gives each namespace an
icon and colour, but only for the built-in namespaces, from a table in the theme module; a user who
registers their own namespace and commands cannot give it an icon, colour, label or doc. The same
gap affects command pickers (namespace groups show the bare name) and any future tool palette.
Workaround: the theme table can be overridden by configuration.

## Expected behaviour

A namespace can be described once, by whoever registers its commands: label, doc, icon reference
(see `ICONS-CANNOT-BE-ASSETS-OR-UNICODE`), colour, and possibly a realm. The registry exports it
with the commands (`specs/command_registry.yaml`), and the editors, command picker and palettes read
it, falling back to the theme's built-in table and then to defaults. Related (weak overlap):
`COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM`, which also changes how the registry is organised.

## Discovery

Raised by Orest on 2026-11-21 while designing card headers and icons for the query editor
(`specs/design/query-editor-ergonomics/reference-design.md` §2.4 and §7).
