# Phase 1: High-level design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question - registration syntax:** command-level hints need a
  stable `register_command!` spelling. The existing `hint key: "value"` grammar is parameter-level
  only, and is parsed and discarded (`ParameterStatement::Hint`, `// TODO: Implement hints` in
  `liquers-macro/src/registration.rs` ≈1057).
- **Explanation:** The metadata field and its serde behaviour are clear, and the macro can carry
  the same JSON string values. A public macro syntax choice remains, because it affects every Rust
  command author.
- **Open questions:**
  - **Proposed resolution - registration syntax:** add a command statement `hint key: "value"`,
    reusing the parameter statement grammar and storing a JSON string. Keep richer JSON values out
    of this small change.
  - **Open design question - merge:** merge with `argument-info-description` (per-argument prose)
    as one design, since the two issues are mirror images (recommended, M3).

## Problem and outcome

`ArgumentInfo` has a serializable free-form `hints` map (`command_metadata.rs` ≈553), but
`CommandMetadata` has no equivalent. A command with no arguments has nowhere to carry UI grouping,
icon, toolbar, documentation, or deprecation hints. Add an empty-by-default map with matching serde
omission, a builder, and macro registration support.

Acceptance criteria:

- a manually built and a macro-registered command keep their command hint;
- an empty map is omitted from serialization, so `registry_export`'s `signature_of` and the
  `metadata_version` of every existing command are unchanged, and the committed registry needs no
  regeneration;
- argument hints keep their current behaviour;
- duplicate hint keys follow one documented rule (rejected at expansion).

## Scope and constraints

Affected: `liquers-core` command metadata, `liquers-macro` registration, registry serialization, UI
consumers. Additive. Out of scope: a global hint vocabulary, completing the parameter-level `hint`
arm, and arbitrary JSON literals in the macro.

## Design Dependencies

- `argument-info-description` - **overlaps** (strong): mirror field, same macro grammar change.
  Merge recommended.
- `command-cache-flag` - **overlaps**: removes `CommandMetadata.cache` and regenerates the registry.
  Ship in the same release to regenerate once.
- `command-declaration` - **overlaps** (complete): its declaration JSON must preserve the new field.
- `MACRO-QUERY-VALIDATION-AND-HINTS` (issue, `accepted`, no design) - **overlaps**: completes the
  parameter-level hint arm.

## Documentation assessment

Review `specs/reference/COMMAND_DECLARATION.md`, `specs/reference/REGISTER_COMMAND_FSD.md`,
`specs/guides/COMMAND_REGISTRATION_GUIDE.md` and `CLAUDE.md`'s DSL list. Change only what describes
the new registration surface.

## Consolidated Findings

`CommandMetadata` is serialized to compute `metadata_version`, so a non-empty command hint changes
that version, while empty maps leave every existing version as it is. The registry freshness test
compares signatures, not bytes, which makes the empty-map omission the property to test. The only
public decision is the macro spelling (plus the recommended merge).
