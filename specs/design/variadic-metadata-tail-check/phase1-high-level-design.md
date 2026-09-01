# Phase 1: High-Level Design - Hand-Built Variadic Metadata Tail Validation

## Design Readiness

- **Readiness:** phase2-blocked
- **Leading issue:** **Blocking - validation boundary:** Runtime registration returns a mutable
  `&mut CommandMetadata` before callers finish adding arguments, so it cannot reject a non-final
  variadic argument at the existing registration call.
- **Explanation:** The validation rule is clear, and the previously cited issue-attribution defect
  is fixed at HEAD, but no current API marks metadata construction complete.
- **Open questions:** **Blocking - validation boundary:** Choose an explicit finalize/validate API,
  validate lazily during planning/export, or redesign registration to accept complete metadata.
  These options produce incompatible error timing and public API behaviour.

## Problem and Evidence

The macro now rejects a consuming argument after a `multiple` argument, but hand-built
`CommandMetadata` can still set `multiple` on a non-final parameter. Resolution then lets the
variadic parameter consume all remaining query arguments and starve later declared arguments.

## Expected Behaviour and Acceptance Criteria

Every runtime metadata validation path reports an error when a non-injected argument follows a
`multiple` argument. Injected arguments, including context, remain allowed after the variadic
argument because they do not consume query parameters.

## Affected Systems

Command metadata validation and any hand-built command registry path are affected. Macro parsing
is already handled and should not be changed. Query syntax and planner resolution semantics should
not change.

## Scope and Non-Goals

Scope is adding the missing validation rule and ensuring reachable registration/export paths call
it. Do not redesign variadic values, composite variadics or UI list editors.

## Compatibility, Assumptions and Questions

The change may reject previously accepted invalid hand-built metadata. Assumption:
`CommandMetadata::check()` is the right rule home, but Phase 2 must account for the fact it is not
currently called by registration.

## Documentation Assessment

Small maintenance may be needed in `specs/reference/REGISTER_COMMAND_FSD.md` or
`specs/guides/COMMAND_REGISTRATION_GUIDE.md` to state the runtime metadata rule. No new guide.

## Design Dependencies

- `requires` `variadic-arguments-declaration`: completed macro validation defines the tail rule and
  leaves only hand-built metadata in this design.
- `overlaps` `command-declaration`: declaration conversion builds complete metadata and could host
  one validation call, but it cannot protect all direct registry mutation paths.

## Review

The rule remains scoped, but acceptance cannot be written truthfully until the repository chooses
when mutable metadata becomes final. Example: checking inside `register_command` sees zero arguments
and passes; a later caller adds `multiple a, b`, recreating the defect after validation.
