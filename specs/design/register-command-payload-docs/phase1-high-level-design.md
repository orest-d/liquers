# Phase 1: High-Level Design - Documenting `payload:`, `expires:` and `version:`

## Design Readiness

- **Readiness:** ready
- **Leading issue:** None. Leading source: `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED`.
- **Explanation:** All three statements' syntax and effects are fixed by the macro
  (`liquers-macro/src/registration.rs`) and asserted by existing tests. This is a documentation-only
  change describing what exists. `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` was
  merged in on 2026-10-05 (maintainer decision after review).
- **Open questions:** None

## Problem and Evidence

`register_command!` accepts three metadata statements that `specs/reference/REGISTER_COMMAND_FSD.md`
§Metadata Statements (table ≈355-366), `CLAUDE.md`'s DSL Syntax Reference (metadata list ends at
`volatile:`) and `specs/guides/COMMAND_REGISTRATION_GUIDE.md` do not list:

| Statement | Parse (`registration.rs`) | Effect (emitter ≈1317-1347) | Existing test |
|---|---|---|---|
| `payload: required` / `payload: none` | bare identifier (≈839) | `required` sets `cm.payload_required = Required` **and** `cm.volatile = true`; `none` emits nothing | `volatility_integration.rs::test_payload_required_sets_metadata_and_volatile` (≈283) |
| `expires: "<spec>"` | string literal (≈881) | `cm.expires = "<spec>".parse()?` — parsed when the command is **registered**, so a bad spec is an `Err` from `register_command!`, not a compile error | `expiration_integration.rs` (`"in 5 min"`, `"immediately"`, `test_register_command_expires_in_plan`) |
| `version: auto \| now \| "<text>" \| <integer>` | identifier, string or integer (≈885-916) | sets `cm.impl_version`: `auto` = hash of the function's source, which **requires** the function to carry `#[liquers_macro::command_version]` (it generates `<fn>__VERSION_()`); `now` = `Version::from_time_now()` at registration, so it changes on every start; a string is BLAKE3-hashed; an integer is used as is | `registration.rs` `test_command_signature_with_version_*`, `test_command_registration_with_version_*` |

`specs/reference/PAYLOAD_GUIDE.md` already documents `payload: required` (quick start ≈41, "Declare
it, or lose it" ≈79-89), so the payload issue's "documented nowhere" is out of date. The
`command_version` attribute is documented nowhere either; a TODO in
`design/dependency-management/phase4-implementation.md` (≈838) asked for exactly this.

## Expected Behaviour and Acceptance Criteria

1. The FSD's metadata-statement table has rows for `payload:`, `expires:` and `version:` with their
   accepted forms and effects; the example block shows all three.
2. The FSD states that `payload: required` also sets `volatile`, that the value is a bare identifier,
   and links `PAYLOAD_GUIDE.md` rather than restating it; §Injected Parameters links the statement
   as the declaration that pairs with an injected `E::Payload`.
3. The FSD states that `expires:` is checked at registration, and links the expiration grammar
   (`reference/api/DOC_08_RECIPES_PLANS.md` §Finalization and expiration) instead of restating it.
4. The FSD documents `version: auto` together with `#[liquers_macro::command_version]`, and warns
   that `version: now` changes on every start (so every dependent re-evaluates after a restart).
5. `CLAUDE.md`'s metadata list includes `payload:`, `expires:` and `version:`.
6. `COMMAND_REGISTRATION_GUIDE.md` lists the three statements in §Macro DSL Syntax and shows two
   short recipes: a command that needs the payload, and a versioned command (`#[command_version]` +
   `version: auto`).
7. Every snippet added compiles as written (checked against existing tests).

## Affected Users

Command authors and agents following `CLAUDE.md`. No code.

## Scope and Non-Goals

Non-goals: changing the macro; restating `PAYLOAD_GUIDE.md`'s inheritance rules or the expiration
grammar; documenting `metadata_version` (owned by `COMMAND_DECLARATION.md`).

## Documentation Assessment

This *is* the documentation change. `REGISTER_COMMAND_FSD.md` and `COMMAND_REGISTRATION_GUIDE.md`
get History rows and `reviewed:` bumps (§9.2); `CLAUDE.md` has no History table. No new document.

## Design Dependencies

| Relationship | Target | Effect |
|---|---|---|
| owns (leading) | `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` | `payload:` |
| owns | `REGISTER-COMMAND-EXPIRES-AND-VERSION-STATEMENTS-UNDOCUMENTED` | `expires:`, `version:`, `#[command_version]` |
| overlaps | design `context-title-description` | also edits `COMMAND_REGISTRATION_GUIDE.md`; History-row conflict only |
| overlaps | design `dependency-management` (complete) | its Phase 4 TODO about `command_version` is resolved here |

## Consolidated Findings

- `PAYLOAD_GUIDE.md` stays the canonical payload explanation; the other documents point to it.
- Every value form differs from its neighbours: `payload` takes a bare identifier, `expires` a string
  checked at runtime, `version` four forms. The FSD must say which, because every other statement
  takes a literal.
- `version: auto` without `#[command_version]` fails to compile (`cannot find function
  <fn>__VERSION_`); the guide recipe shows both together.

## Review

Small, verifiable against the macro source and existing tests. Scope widened on 2026-10-05.
