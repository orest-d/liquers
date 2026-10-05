# Phase 1: High-Level Design - Documenting `payload: required`

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None
- **Explanation:** The statement's syntax and effect are fixed by the macro
  (`liquers-macro/src/registration.rs`) and asserted by an existing test; this is a
  documentation-only change describing what exists.
- **Open questions:** None

## Problem and Evidence

`register_command!` accepts `payload: required` / `payload: none`
(`CommandSignatureStatement::PayloadRequired`, parsed ≈839, emitted ≈1306: `required` sets
`cm.payload_required = PayloadRequirement::Required` **and** `cm.volatile = true`; `none` emits
nothing). Tested by `test_payload_required_sets_metadata_and_volatile`
(`liquers-core/tests/volatility_integration.rs` ≈283).

Verified at HEAD — the issue is partly out of date, partly confirmed:

| Document | State |
|---|---|
| `specs/reference/PAYLOAD_GUIDE.md` | **does** document it (quick-start example ≈41; "Declare it, or lose it" ≈79-89) — the issue's "documented nowhere" is no longer true |
| `specs/reference/REGISTER_COMMAND_FSD.md` §Metadata Statements (≈339-366) | table omits `payload:` — confirmed |
| `CLAUDE.md` DSL Syntax Reference (≈366) | metadata list ends at `volatile:` — confirmed |
| `specs/guides/COMMAND_REGISTRATION_GUIDE.md` | no mention of `payload:`; §Macro DSL Syntax defers to the FSD — confirmed |

## Expected Behaviour and Acceptance Criteria

1. The FSD's metadata-statement table has a `payload: required` / `payload: none` row: bare
   identifier (not a string, not a bool), meaning, and "`required` also sets `volatile`"; and the
   example block shows it.
2. The FSD's §Injected Parameters links the statement as the declaration that pairs with an
   injected `E::Payload` parameter, and to `PAYLOAD_GUIDE.md`.
3. `CLAUDE.md`'s metadata list includes `payload:`.
4. `COMMAND_REGISTRATION_GUIDE.md` lists `payload` in the §Macro DSL Syntax bullet list and
   shows the one-line declaration next to an injected payload parameter, linking
   `PAYLOAD_GUIDE.md` rather than restating it.
5. Every example `register_command!` snippet added compiles as written (checked against the
   macro grammar and the existing test).

## Affected Users

Command authors and agents following `CLAUDE.md`. No code.

## Scope and Non-Goals

In scope: the three documents above. Non-goals: `expires:` and `version:` statements, which the
macro also accepts (≈881, ≈885) and which the FSD table also omits — filed separately as
`REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED`, so this design stays one source;
changing the macro.

## Documentation Assessment

This *is* the documentation change: two `guides/`/`reference/` documents get History rows and
`reviewed:` bumps (§9.2); `CLAUDE.md` has no History table. `PAYLOAD_GUIDE.md` is unchanged
(already correct).

## Design Dependencies

None.

## Consolidated Findings

- `PAYLOAD_GUIDE.md` is the canonical explanation; the other documents should point to it, not
  duplicate its inheritance rules — one place to keep correct.
- The value is a bare identifier: `payload: "required"` or `payload: true` is a compile error.
  The FSD must say so, since every neighbouring statement takes a literal.
- `payload: none` is accepted and emits nothing (default); documented as such so readers do not
  think it un-sets a requirement elsewhere.
- Filing: the `expires:` / `version:` omission is a separate defect found while verifying this
  one.

## Review

Small, verifiable against the macro source and an existing test.
