---
id: COMMAND-REGISTRY-NOT-ORGANIZED-BY-REALM
kind: issue
title: The command metadata registry is a flat list, so a realm cannot have its own default namespaces
status: draft
priority: P3
complexity: L
area: [core/commands, core/plan]
design:
created: 2026-11-01
github:
---

## Problem

`CommandMetadataRegistry` (`liquers-core/src/command_metadata.rs`) holds every command in one
`Vec<CommandMetadata>` and one `default_namespaces: Vec<String>` that applies to every realm. The
realm is only a field of each command's key. Nothing in the registry stands for a realm, so
nothing per realm can be declared — in particular a realm's own default namespace chain. The code
already notes it: `// TODO: Refactor CommandMetadataRegistry to use realm/ns hierarchy and CommandKey`.
Lookup is also linear: `find_command` scans the whole list for each namespace tried.

**Example.** A registry with Python commands in realm `py`, namespace `img`, and Polars commands in
the main realm, namespace `pl`. The main realm should search `pl, root` and an action in a `-py/…`
segment should search `img, root`. Today one chain serves both, so either `-py/…` actions also
search `pl` or main-realm actions also search `img`.

## Impact

Multi-realm work (`MULTI-REALM-DISPATCH`, `REGISTRY-PUBLICATION`, `CROSS-REALM-TRANSFER-PLANNING`)
imports another realm's registry into this one, and that realm's namespace conventions come with
it. Workaround: write `ns-…` in every segment of the other realm.

## Expected behaviour

The registry is organised by realm, and each realm carries its own `default_namespaces` (and
whatever else turns out to be per realm). Options, not chosen here: a `realm → { commands,
default_namespaces }` structure replacing the flat list, with serde compatibility for exported
registries; or a side map `realm_default_namespaces` beside the existing fields, a smaller step
that was drafted in `design/plan-default-namespaces/` and taken out of that design on 2026-11-01 at
the user's request, to be decided here instead.

A design is needed (complexity L).

## Discovery

User review of `design/plan-default-namespaces/` (PR #236), 2026-11-01: per-realm defaults were
requested, then deferred to a registry reorganisation by realm.
