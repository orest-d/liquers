---
id: ENUM-ALTERNATIVES-HAVE-NO-LABEL
kind: issue
title: An enum alternative has an alias and a value but no label or description
status: draft
priority: P3
complexity: M
area: [core/commands, macro]
design:
created: 2026-10-20
github:
---

## Problem

`EnumArgumentAlternative` (`liquers-core/src/command_metadata.rs`) has `alias` and `value` only;
the struct carries `// TODO: Label and description for alternatives`. A UI selector for
`method: String (enum: ["nearest", "lanczos3"])` can show only the query spelling, and
`register_command!`'s `enum:` syntax has nowhere to put a label.

## Impact

Enum choices in UIs and MCP schemas are shown as raw aliases. Cosmetic; workaround: describe the
alternatives in the argument's `description`.

## Expected behaviour

Optional `label` and `description` on an alternative, omitted from serialization when empty (as
`ArgumentInfo::description` is), and a macro spelling for them. The registry export and the MCP
schema carry them.

## Discovery

Seen during the 2026-10-20 bulk design of `COMMAND-METADATA-ENHANCEMENTS`; the earlier draft
`design/register-command-enum/` listed it as a non-goal of its first iteration.
