---
id: COMMAND-CONTEXT-PARAM-ORDER
kind: issue
title: The context parameter must come last, as a workaround
status: closed
priority: P2
complexity: M
area: [core/commands, macro]
design: context-param-order
created: 2026-08-08
github:
---
## Problem

`register_command!` requires the `context` parameter to be declared **last**, and the surrounding
guidance repeats the constraint as a rule to follow rather than a defect to fix. It is a workaround
for a parameter-index bug: the argument index used when resolving parameters does not account for
`context` appearing anywhere other than at the end.

Findings and the original fix proposal are archived as
`specs/archive/2026-09-02-context-param-order-{findings,solution}.md`.

**Update 2026-10-10.** The parameter-index fix has landed (`CommandSignature::extract_all_parameters`
in `liquers-macro/src/registration.rs` advances the slot only for real arguments), so `context`
anywhere *after* the state now works. Still open: `context` before the state does not parse, a
duplicate `context` fails inside the expansion, and the guides still prescribe "context last". The
design `specs/design/context-param-order/` was rewritten to cover these; the goal is `context` at
any position.

## Impact

Every command signature is constrained by an implementation detail, and the constraint is easy to
violate — the failure is a runtime mismatch rather than a compile error.

## Expected behaviour

`context` may appear at any position, or the macro rejects a misplaced one at compile time.

## Discovery

Recorded in `specs/design/context-param-order/` since before the migration. It was **never in
`ISSUES.md`** (now `specs/archive/2026-08-08-issues.md`), yet `.claude/skills/rust-best-practices/references/anti-patterns.md` and
`liquers-designer/references/liquers-patterns.md` both cited `specs/archive/2026-08-08-issues.md` for it — a dangling
reference this issue now resolves.

## Resolution

`register_command!` accepts `context` at any position, recommended last or immediately before a
`multiple` argument; misplaced or duplicate entries are compile-time errors. Implemented and
documented by `design/context-param-order/` on 2026-10-10.
