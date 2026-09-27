---
id: ARGUMENT-INFO-HAS-NO-DESCRIPTION
kind: issue
title: ArgumentInfo has no per-argument description
status: draft
priority: P3
complexity: S
area: [core/commands, macro]
design:
created: 2026-09-24
github:
---
# `ArgumentInfo` has no per-argument description

## What is missing

`ArgumentInfo` (`liquers-core/src/command_metadata.rs:517`) carries `name`, `label`, `default`,
`argument_type`, `multiple`, `injected`, `gui_info`, `hints` and `presets`. It carries **no prose**.

`CommandMetadata` has `doc` (`:975`) — the only `pub doc` in the file — so a command can be
explained but a **parameter cannot**. The nearest available places are both wrong for it:

- `label` is a two-or-three-word display name, sized for a form label, not an explanation.
- `hints` is an untyped `serde_json::Map`, so anything put there has no agreed key and nothing reads
  it by convention.

## Why it matters

A parameter's meaning is exactly what a caller needs and cannot infer:

- **A UI** can render a label but has nothing for a tooltip or help text.
- **An agent** choosing arguments from `command_registry.yaml` sees a name, a type and a default. For
  anything whose meaning is not obvious from its name, that is guesswork — and the registry is
  precisely the artifact an agent reads to decide how to call a command.
- **`register_command!`** has a `doc:` statement for the command and no equivalent per argument, so
  an author who wants to explain a parameter has nowhere to write it.
- **Presets** are documented as doubling for documentation (*"They can also serve as a documentation
  for the argument"*), which is an admission that the gap is being worked around.

## Shape of a fix

A `description: String` field beside `label`, `#[serde(default)]` so existing declarations and every
`command_registry.yaml` keep deserializing unchanged, plus a `with_description` builder matching the
existing `with_label` (`:713`), and a macro statement so the supported registration path can set it.

Regenerating `specs/command_registry.yaml` afterwards is a no-op for commands that do not use it,
since an empty `description` serializes away like the other defaulted fields.

## How it was found

Comparing `ArgumentInfo` against `FieldSchema` in `specs/design/record-streams/` — the two describe
different things but overlap on "a named, typed slot a person eventually sees", and the comparison
was to decide how far they should align. `label` aligned exactly, down to its
`name.replace("_", " ")` default. `description` did not, because only one of them has it: the record
design takes the field, and the command side does not.

## Relation to `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`

That issue is this one's **mirror image**, and the pair describes one asymmetry rather than two
problems:

| | Command level | Argument level |
|---|---|---|
| Prose | `CommandMetadata::doc` ✓ | **missing — this issue** |
| Free hints | **missing — `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS`** | `ArgumentInfo::hints` ✓ |

Each level has exactly what the other lacks. That issue is `in_progress` with a design folder
(`command-metadata-command-hints`), so whoever is working it is already in this code and holding the
symmetry question. **These should be settled together**, and the answer is probably that both levels
carry both — a description and a hint bag — rather than each carrying one.
