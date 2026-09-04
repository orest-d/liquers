---
id: REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED
kind: issue
title: The `payload: required` metadata statement is implemented but documented nowhere
status: draft
priority: P2
complexity: S
area: [docs, core/commands, macro]
design:
created: 2026-09-03
github:
---

## Problem

`register_command!` accepts a `payload: required` / `payload: none` metadata statement. It is
implemented (`liquers-macro/src/registration.rs:799`, parsed at `:1850`, emitted at `:1308`), it
sets both `payload_required` and `volatile` on the command metadata, and that behaviour is tested
(`liquers-core/tests/volatility_integration.rs:286`,
`test_payload_required_sets_metadata_and_volatile`).

No document mentions it:

- `specs/reference/REGISTER_COMMAND_FSD.md` §"Metadata statements" (`:337`) has a table of every
  statement at `:360`. It lists `volatile: true/false` and omits `payload:` entirely. The only
  mention of a payload in that file is the *injected* `E::Payload` parameter (`:160`), which is a
  different mechanism.
- `CLAUDE.md`'s DSL Syntax Reference (`:355`) ends its metadata list at `volatile:`.
- `specs/guides/COMMAND_REGISTRATION_GUIDE.md` does not mention a payload at all.

So the statement is discoverable only by reading the macro's parser or by finding the one test.

## Impact

A command author who needs a payload cannot find the declaration that makes it work, and is likely
to reach for `volatile: true` plus an injected payload parameter instead — which produces a command
that runs without a payload and reads `None`, exactly the failure the `payload: required` gate
exists to prevent. `interpreter::apply_plan` calls that gate "the authoritative gate"; a gate
nobody knows how to arm is not authoritative.

Low severity because the mechanism works and its absence is silent rather than wrong. The fix is
three table rows.

## Expected behaviour

`REGISTER_COMMAND_FSD.md`'s metadata-statement table lists `payload: required` / `payload: none`
with its meaning and its implication (`required` also sets `volatile`), `CLAUDE.md`'s DSL list
includes it, and `COMMAND_REGISTRATION_GUIDE.md` shows the one-line declaration next to the
injected-payload parameter it pairs with.

## Discovery

Found on 2026-09-03 while drafting the Phase 3 test plan for
`specs/design/evaluate-path-consolidation/`: a test needed a payload-requiring command, and the
declaration had to be recovered from the macro source because no document names it.
