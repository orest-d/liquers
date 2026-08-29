# Phase 1: High-Level Design - Hand-Built Variadic Metadata Tail Validation

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

## Review

The design is a scoped follow-up to a completed broader design and keeps the issue's remaining
behaviour testable without reopening macro work.
