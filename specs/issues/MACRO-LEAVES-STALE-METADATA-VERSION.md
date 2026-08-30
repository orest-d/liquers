---
id: MACRO-LEAVES-STALE-METADATA-VERSION
kind: issue
title: A macro-registered command's metadata_version is computed from its bare key, not its content
status: draft
priority: P1
complexity: S
area: [core/commands, macro, core/assets]
design:
created: 2026-08-30
github:
---
## Problem

`CommandMetadata::metadata_version` is documented as *"Version of the metadata structure content.
This is computed from JSON serialization and managed by the metadata registry"*
(`liquers-core/src/command_metadata.rs:1002-1006`). For every command registered through
`register_command!` it is not: it reflects only the command's realm, namespace and name.

The cause is an ordering problem in registration. `CommandRegistry::register_command`
(`commands.rs:613-625`) inserts a bare skeleton and returns a handle to it:

```rust
let command_metadata = CommandMetadata::from_key(key.clone());
self.command_metadata_registry.add_command(&command_metadata);   // version computed HERE
self.executors.insert(key.clone(), Arc::new(Box::new(f)));
```

`add_command` computes `metadata_version` from what it is given
(`command_metadata.rs:1186-1193`). The macro then mutates the returned `&mut CommandMetadata`,
filling in the label, the arguments, their types, defaults and GUI hints — and **nothing recomputes
the version**. `update_command_metadata_version` and `update_all_metadata_versions` exist for
exactly this (`:1220`, `:1232`) but have no caller anywhere outside their own unit tests.

## Evidence

Measured while writing the declaration/macro parity test for `design/command-declaration/`. One
command, `repeat(state, count: i64)`, registered through the macro:

```
stored     Version(125565375696920796040135898744284505298)
recomputed Version(104777844030880026506816284629315674945)   <- after update_command_metadata_version
```

The recomputed value is byte-identical to the one the declaration path produces for the same
content, which is what confirms the diagnosis: the content agrees, only the stored version is stale.

## Why it matters

The invariant `metadata_version` exists to provide — that it changes when the command's metadata
changes — does not hold for the primary registration path. Two commands sharing a key but differing
in every argument hash identically, and editing a command's signature does not move its version.

`AssetManager` records a dependency on the command's `metadata_version`
(`liquers-core/src/assets.rs:3240-3243`), so anything downstream that reasons about whether a
command changed is reasoning from a value that does not track the command.

It has stayed invisible because `metadata_version` is `#[serde(skip)]`, so it never appears in
`specs/command_registry.yaml` and no export comparison can catch it.

## Fix direction

Recompute after mutation. The narrow fix is for `register_command!` to call
`update_command_metadata_version` once it has finished filling the metadata in. The broader and
probably better fix is to make it impossible to get wrong: have registration take the completed
metadata rather than handing back a mutable handle to an already-inserted skeleton, so there is no
window in which a stored command is half-built.

Either way, add a test that a command's `metadata_version` changes when its argument list changes —
that is the property, and nothing currently asserts it.

## Priority rationale

**P1.** It meets the `DOCS_STRUCTURE_GUIDE.md` §4.4 wording for P0 — a documented feature that does
not work — and it feeds a value into dependency tracking. It is filed at P1 rather than P0 because
no incorrect *result* has been demonstrated: the consequence is a missed invalidation, which needs
confirming against the expiration machinery before the stronger claim is made. Worth raising to P0
if that confirmation lands.

## Related

- `REGISTRY-IMPL-VERSION-DRIFT-UNDETECTED` — the neighbouring field, and the same class of "a
  version that does not track what it names". Worth reading together.
- `design/command-declaration/` — found this. Its INT02 parity test asserts on the recomputed
  version and says why, so it documents the bug rather than working around it silently.

## Verification

`register_command!` two commands that differ only in their argument list; their `metadata_version`
values differ. And the stored version of a macro-registered command equals its recomputed version.
