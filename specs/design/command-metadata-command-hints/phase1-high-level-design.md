# Phase 1: High-level design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - registration syntax:** command-level hints need a stable `register_command!` spelling; the existing `hint key: "value"` grammar is only parameter-level and currently discarded.
- **Explanation:** The metadata field and serde behaviour are clear, and the macro can carry the same JSON string values. A public macro syntax choice remains visible because it affects every Rust command author.
- **Open questions:**
  - **Proposed resolution - registration syntax:** add a command statement `hint key: "value"`, reusing the existing parameter statement grammar and storing a JSON string. Keep richer JSON values out of this small change.

## Problem and outcome

`ArgumentInfo` has a serializable free-form `hints` map, but `CommandMetadata` has no equivalent. A command with no arguments therefore has nowhere to carry UI grouping, icon, toolbar, documentation, or deprecation hints. Add an empty-by-default map with matching serde omission, a builder, and macro registration support.

Acceptance criteria: a manually built and macro-registered command retains its command hint; empty hints do not alter exported registry bytes; argument hints retain their current behaviour; duplicate hint keys follow one documented rule.

## Scope and constraints

Affected systems are `liquers-core` command metadata, `liquers-macro` registration, registry serialization, and UI consumers. This is additive and does not migrate the committed registry. The map is metadata, not execution or query semantics. Do not design a global hint vocabulary, argument-level hint completion, or arbitrary macro JSON literals.

## Design Dependencies

- `command-declaration` - **overlaps**: its declaration JSON already permits metadata extension; it should preserve the new field without adding a second representation.
- `MACRO-QUERY-VALIDATION-AND-HINTS` - **overlaps**: it completes the existing parameter-level macro hint arm; this design adds the command-level analogue independently.

## Documentation assessment

Review `specs/reference/COMMAND_DECLARATION.md`, `specs/guides/COMMAND_REGISTRATION_GUIDE.md`, and `specs/command_registry.yaml`; change only documents that describe the new registration surface.

## Consolidated Findings

`CommandMetadata` is already JSON-serialized to calculate `metadata_version`, so a non-empty command hint correctly changes that version while empty existing metadata remains byte-identical. The only public decision is the macro spelling; the recommended string-only command statement mirrors the parameter syntax and avoids inventing a value-language.
