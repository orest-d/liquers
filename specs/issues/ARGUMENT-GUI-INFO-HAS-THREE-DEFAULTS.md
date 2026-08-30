---
id: ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS
kind: issue
title: An argument's default gui_info differs depending on how the command was registered
status: draft
priority: P2
complexity: S
area: [core/commands, macro, web]
design:
created: 2026-08-30
github:
---
## Problem

`ArgumentInfo::gui_info` has three different defaults, one per registration path, for the same
argument declared the same way:

| Path | Default | Source |
|---|---|---|
| `register_command!` | `TextField(20)` | `liquers-macro/src/registration.rs:1710` |
| `ArgumentInfo::any_argument` — what `liquers-web` builds arguments with | `TextField(40)` | `liquers-core/src/command_metadata.rs:422` |
| `serde`, when a document omits the field | `None` | `ArgumentGUIInfo`'s `#[default]` |

So the same command, registered from Rust and from JavaScript, produces different metadata. Since
`metadata_version` is computed from the stored content
(`command_metadata.rs:1036`), the two also carry different versions, and a UI renders the argument
with a different width depending on where it came from.

## How it was found

Writing the parity test for `design/command-declaration/` — a command registered through
`register_command!` compared against the same command declared. Everything else matched; the
argument's `gui_info` did not:

```
MACRO:       …"arguments":[{"name":"count",…,"gui_info":{"TextField":20}}]…
DECLARATION: …"arguments":[{"name":"count",…,"gui_info":{"TextField":40}}]…
```

The declaration path deliberately follows `any_argument`, because that is what
`liquers-web` produces today and changing it would move every existing JavaScript command's
`metadata_version` and re-expire its dependent assets. So the divergence is not introduced by that
design; it predates it and the design simply cannot resolve it unilaterally.

## Why it matters

`gui_info` is a hint, so nothing is broken at runtime. What is broken is the invariant that equal
content gives an equal version: two identical commands are not identical, and a test asserting they
are has to exclude the field. That is exactly the kind of exclusion that hides a real divergence
later.

The macro's `TextField(20)` also looks accidental rather than chosen — it is a single flat literal
set before parsing, not a rule about the argument, and it is not type-aware despite `IntegerField`
and the other variants existing.

## Fix direction

Pick one default and use it everywhere:

1. **Agree a single value.** `TextField(40)` matches `any_argument` and requires no change to
   `liquers-web`; adopting `TextField(20)` instead would re-version every JavaScript-registered
   command. That asymmetry argues for 40.
2. **Or make it type-aware**, which is what the flat literal is standing in for: `IntegerField` for
   `int`, a wider `TextField` for `string`, and so on. Better, and a bigger change — it moves every
   existing command's `metadata_version` at once, so it wants doing deliberately and in one go
   rather than drifting into place.
3. Either way, `ArgumentGUIInfo`'s serde default should stop being a third answer:
   `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` is the same shape of defect on a neighbouring
   field, and both want settling together.

## Related

- `STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE` — the constructor-versus-serde default
  disagreement on `state_argument`; the same class of defect and worth one fix.
- `design/command-declaration/` — surfaced this; its parity test excludes `gui_info` and says why.
- `MACRO-QUERY-VALIDATION-AND-HINTS` — touches the macro's per-argument statement handling.

## Verification

One command, registered through `register_command!` and through a declaration, produces byte-equal
metadata including `gui_info` and `metadata_version`, with no field excluded from the comparison.
