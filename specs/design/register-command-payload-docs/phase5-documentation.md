# Phase 5: Documentation - Documenting `payload:`, `expires:` and `version:`

**Status: executed 2026-10-07**, after implementation (Wave 3 step 20 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Phase 4 steps 1-4 complete
- [x] User and review comments answered or incorporated
- [x] Documentation consistent with the macro as implemented

## Documentation Plan

### New Reference Documents

None.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `reference/REGISTER_COMMAND_FSD.md` | three table rows, example block, §Injected Parameters link, "Implementation versions" subsection; History row + `reviewed:` |
| `guides/COMMAND_REGISTRATION_GUIDE.md` | DSL bullet, two task recipes; History row + `reviewed:` |

`CLAUDE.md` is updated too but is outside `specs/` and carries no History table.

### Candidates Considered and Discarded

By area (`macro`, `core/commands`, `docs`): `COMMAND_DECLARATION.md` (owns `metadata_version`, says
`impl_version` is supplied at registration — still true), `PAYLOAD_GUIDE.md` (already correct),
`LANGUAGE-INTEGRATION_GUIDE.md`, `DOC_08_RECIPES_PLANS.md` (linked, not changed).

### Links and Capability Map

None needed: the FSD is already the map's target for the macro.

### Issues to Close

`REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` (note that `PAYLOAD_GUIDE.md` already covered it)
and `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` → `status: closed` with resolutions.

## Implementation Summary

Documentation only, as designed. The grammar was re-verified against
`liquers-macro/src/registration.rs` (parse arms for `payload`, `expires`, `version`; emitters
`payload_required_code`, `expires_code`, `impl_version_code`) and the claim that the version feeds
freshness against `load_command_versions_sync` (`liquers-core/src/assets.rs`).

## Documentation Delivered

- `reference/REGISTER_COMMAND_FSD.md`: example block with all three statements and
  `#[command_version]`; a sentence naming the three non-literal value forms; table rows for
  `payload:`, `expires:`, `version:`; new §Implementation versions; §Injected Parameters links
  `payload: required`. History row, `reviewed:` bumped.
- `guides/COMMAND_REGISTRATION_GUIDE.md`: the DSL bullet lists every statement; new sections
  "Commands that need the payload" and "Versioning a command so its results expire when its code
  changes", with the Phase 3 snippets. History row, `reviewed:` bumped.
- `CLAUDE.md`: the DSL metadata list names `payload:`, `expires:` and `version:`.

## Issues Filed

None.

## Important Learning

A `version:` string is hashed at compile time (BLAKE3 in the macro), not at registration.

## Conformance and Remaining Work

Conforms to Phases 1–4. The Phase 4 TODO in `design/dependency-management/` about documenting
`command_version` is answered by §Implementation versions. No remaining work.

## Validation

`cargo test -p liquers-macro version`; `cargo test -p liquers-core --test volatility_integration
test_payload_required`; `cargo test -p liquers-core --test expiration_integration
test_register_command_expires_in_plan`; `python3 scripts/docs_index.py --check`.
