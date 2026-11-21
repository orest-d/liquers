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
Workaround once `query-editor-ergonomics` ships: the theme configuration can name an icon and colour per namespace.

## Expected behaviour

The command metadata registry (`CommandMetadataRegistry`) holds a **namespace metadata** entry
beside the commands, declared by whoever registers the namespace's commands (a builder call or a
`register_command!`-style statement), and exported with them in `specs/command_registry.yaml`.
What it needs, decided with Orest on 2026-11-21:

| Field | Need | Notes |
|---|---|---|
| `name` | required | The namespace id (`pl`), with its realm once the registry is organised by realm (`COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM`) |
| `label` | required for display | Human name ("Polars DataFrame"), used by card headers, outlines and the command picker |
| `doc` | optional | What the namespace is for; tooltip and picker description |
| `icon` | optional | An icon reference: a Phosphor name now, later an asset or Unicode (`ICONS-CANNOT-BE-ASSETS-OR-UNICODE`) |
| `color` | optional, open | Recommended as a **named hue** from the theme palette (`teal`, `violet`), not a hex value, because one colour cannot suit a dark and a light theme; the theme maps the hue to each theme |

Lookup when drawing a namespace: theme overrides (configuration) → namespace metadata in the
registry → the theme's built-in table for the bundled namespaces → defaults (neutral colour, a
monogram). An undeclared namespace keeps working. The `query-editor-ergonomics` design implements
the theme overrides and the built-in table; this issue adds the registry entry.

## Discovery

Raised by Orest on 2026-11-21 while designing card headers and icons for the query editor
(`specs/design/query-editor-ergonomics/reference-design.md` §2.4 and §7).
