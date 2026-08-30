---
id: COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS
kind: feature
title: A usage hint can be attached to an argument but not to a command
status: draft
priority: P3
complexity: S
area: [core/commands, lib/ui]
design:
created: 2026-08-30
github:
---
## Problem

`ArgumentInfo` carries a free dictionary of usage hints:

```rust
/// Free dictionary of hints for the argument.
/// This may be used e.g. to provide additional hints for the UI.
#[serde(skip_serializing_if = "serde_json::Map::is_empty")]
#[serde(default)]
pub hints: serde_json::Map<String, serde_json::Value>,
```
(`liquers-core/src/command_metadata.rs:399-403`)

`CommandMetadata` has no equivalent. It is the only `pub hints` field in the file. So a hint can be
attached to one argument but not to the command as a whole.

## Why it matters

The asymmetry has no stated reason, and the command level is where several natural hints belong: a
UI category or grouping, an icon, a "show this in the toolbar" flag, a documentation anchor, a
deprecation note. Each is a fact about the command, not about any one of its arguments.

Today the only way to express one is to attach it to an arbitrary argument, which is wrong wherever
it is put and impossible for a command with no arguments at all.

`CommandPreset` and `next` show that `CommandMetadata` is already expected to carry UI affordances,
so an extension point at that level is consistent with what the type is for.

## Expected behaviour

A `hints` field on `CommandMetadata` mirroring the one on `ArgumentInfo`, with the same serde
treatment so an empty map does not appear in the exported registry:

```rust
#[serde(skip_serializing_if = "serde_json::Map::is_empty")]
#[serde(default)]
pub hints: serde_json::Map<String, serde_json::Value>,
```

`register_command!` would need a statement to set it — the macro already accepts a `hint` statement
at the *parameter* level (`CommandParameterStatement::Hint`, `registration.rs:1057`), marked
`TODO: Implement hints`, so the command-level counterpart is the same shape of work.

## Cost and caution

Adding a field changes `metadata_version` for nothing else, since the field is empty for every
existing command and `skip_serializing_if` keeps it out of `command_registry.yaml`. The
`registry_export` test compares signatures, so it should stay green — worth confirming rather than
assuming, since the exported file is committed.

## Related

- `design/command-declaration/` — surfaced this. Its `COMMAND_DECLARATION.md` §6 distinguishes usage
  hints (metadata) from registration hints (declaration-only) and has to note that the first cannot
  be expressed at command level. Not a blocker for it: a declaration can set argument-level hints
  today, and would gain the command level for free if this lands.
- `MACRO-QUERY-VALIDATION-AND-HINTS` (P3, `accepted`) — covers the unimplemented parameter-level
  `hint` statement in the macro; this issue is the command-level field the same work would use.
- `COMMAND-METADATA-ENHANCEMENTS` (P2, `accepted`) — the broader metadata extension effort.

## Verification

A command registered with a command-level hint round-trips it through
`CommandMetadataRegistry`, and `specs/command_registry.yaml` is byte-identical for the existing
command set.
