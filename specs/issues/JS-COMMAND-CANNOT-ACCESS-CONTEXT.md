---
id: JS-COMMAND-CANNOT-ACCESS-CONTEXT
kind: feature
title: A JavaScript command cannot access the execution context
status: draft
priority: P2
complexity: M
area: [web, core/commands]
design:
created: 2026-08-29
github:
---
## Problem

A Rust command declares `context` in `register_command!` and receives a `Context<E>`, giving it
progress reporting, the environment, injected values and the evaluation payload. A JavaScript
command cannot: `register_js_command` is handed a context and discards it.

```rust
// liquers-web/src/command/adapter.rs:165
    _context: Context<E>,
```

`CallableSpec` (`adapter.rs:130`) carries `run`, `state_mode`, `is_async` and `name` — there is no
context in the calling convention, so `call_js_command` has nothing to pass even if it wanted to.

## Why it matters

Context is not a marginal capability. Without it a JavaScript command cannot report progress on a
long operation, cannot read an injected value, cannot reach the payload, and cannot evaluate a
sub-query. That rules out most non-trivial commands, so the JavaScript command surface is limited to
pure transformations of its input in a way the Rust surface is not — an asymmetry that is invisible
from the JavaScript side until an author needs one of those things.

A Python binding will hit the same wall, and for the same reason: there is no portable way to say
"this callable takes a context".

## Fix direction

Two halves, and the second is the substantial one:

1. **Declare it.** The calling convention needs to express whether the callable receives a context —
   in Rust this is `CommandParameter::Context` (`liquers-macro/src/registration.rs:489`), a
   parameter that occupies no argument slot. `COMMAND-DECLARATION-FORMAT`'s `CallingConvention` is
   where the portable form would go, and that design deliberately leaves room for it rather than
   sealing the type.
2. **Implement it.** A JavaScript-visible context object wrapping `Context<E>` — which methods it
   exposes, how an async call from JavaScript back into the evaluator is driven, and how the
   `RefCell`/manager-guard discipline in `adapter.rs`'s module comment survives a re-entrant call,
   are all open. This is the design work; the declaration half is trivial by comparison.

Note `COMMAND-CONTEXT-PARAM-ORDER` (P2, `accepted`): the Rust macro currently requires `context`
last as a workaround. A JavaScript convention should not inherit that constraint by accident —
positional context in a JavaScript call is a choice, not a given, and a leading context argument or
a bound `this` may be better.

## Related

- `COMMAND-DECLARATION-FORMAT` — found this while classifying which of the macro's wrapping
  decisions are portable. Not a blocker for it: that design records context injection as a wrapping
  decision with no JavaScript implementation, and leaves `CallingConvention` open to gaining the
  field later.
- `COMMAND-CONTEXT-PARAM-ORDER` — the Rust-side ordering workaround.
- `POST-INIT-COMMAND-REGISTRATION` — unrelated, but touches the same registration path.

## Verification

A JavaScript command that reports progress and reads an injected value, registered through
`liquers.registerCommand`, observed to do both from a conformance test.
