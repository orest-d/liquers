# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — adds a `pub` field to `ArgumentInfo` and `register_command!`
  syntax, and changes `specs/command_registry.yaml` (rule 4); also needs-decision (rule 5)
- **Leading issue:** **Open design question — settle together with command-level hints.** The
  issue and `COMMAND-METADATA-HAS-NO-COMMAND-LEVEL-HINTS` (design
  `command-metadata-command-hints`, `needs-decision`) are two halves of one asymmetry. The
  maintainer should decide whether to merge the designs (§5.1.1) so one registry regeneration and
  one macro grammar change cover both.
- **Explanation:** The field, serde and builder are mechanical. The macro spelling is a public
  syntax choice for every command author. A working design is specified around the recommended
  spelling.
- **Open questions:**
  1. **Proposed resolution — field name `description`:** matches `FieldSchema::description`
     (records) and the command preset's `description:`. The alternative `doc` would mirror
     `CommandMetadata::doc`.
  2. **Proposed resolution — macro spelling:** an argument option inside the existing parenthesized
     option list: `width: i32 = 80 (label: "Width", description: "Maximum line width in characters")`.
     This reuses a grammar that already holds `label:`/`gui:`/`enum:`.
  3. **Open design question — merge:** recommended to merge with `command-metadata-command-hints`
     (leading source: that feature, since it is already `in_progress`).

## Problem

`ArgumentInfo` (`liquers-core/src/command_metadata.rs`) has `name`, `label`, `default`,
`argument_type`, `multiple`, `injected`, `gui_info`, `hints` and `presets`, but no prose.
`CommandMetadata` has `doc`. UIs have no tooltip text, and agents reading
`specs/command_registry.yaml` see only name, type and default.

## Expected behaviour and acceptance

1. `ArgumentInfo.description: String`, defaulted to empty and omitted from serialization when
   empty. Every committed `command_registry.yaml` deserializes unchanged, and regenerating it
   without new descriptions is byte-identical.
2. `ArgumentInfo::with_description(&mut self, &str) -> &mut Self`, matching `with_label`.
3. `register_command!` accepts `description: "…"` among an argument's options and stores it.
4. `metadata_version` of commands without descriptions is unchanged (follows from 1).
5. The Python binding exposes it as a read-only getter if `ArgumentInfo` is bound there.

## Scope and non-goals

Not in scope: writing descriptions for existing commands, which is a follow-up content task. A
UI tooltip rendering may follow separately.

## Design Dependencies

- `command-metadata-command-hints` — **overlaps** (strong). The same file and macro, with mirrored
  fields. **Recommended merge candidate.**
- `command-cache-flag` — **overlaps**. Also changes `CommandMetadata` and the registry. Order
  matters only for registry regeneration.

## Documentation assessment

- Reference: extend `specs/reference/REGISTER_COMMAND_FSD.md` (argument options table) and
  `specs/reference/COMMAND_DECLARATION.md` if it lists `ArgumentInfo` fields.
- Guide: `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, one example line.
- Update: `CLAUDE.md` DSL syntax reference ("Parameters:" line), one phrase.

## Consolidated Findings

- serde `skip_serializing_if = "String::is_empty"` + `default` keeps both the registry and
  `metadata_version` stable for unchanged commands. That is essential, because a version change
  expires every dependent of every command on the next start.
- The macro's argument option parser already rejects unknown keys. Adding `description` is one
  arm plus the generated `.with_description(..)` call or field assignment.
- `liquers-py/src/command_metadata.rs` wraps `ArgumentInfo`. Check whether it has per-field
  getters, and add `description` if so.
