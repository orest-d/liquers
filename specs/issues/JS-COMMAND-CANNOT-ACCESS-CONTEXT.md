---
id: JS-COMMAND-CANNOT-ACCESS-CONTEXT
kind: feature
title: A JavaScript command cannot access the execution context
status: draft
priority: P2
complexity: M
area: [web]
design: js-command-context
created: 2026-08-29
github:
---
## Problem

A Rust command declares `context` and receives a `Context<E>`: progress and log entries, nested
`evaluate`, cancellation, injected values and the payload. A JavaScript command receives none of
it. `call_js_command` (`liquers-web/src/command/adapter.rs`) takes the `Context<E>` only to read its
`EnvRef` for value conversion; `CallableSpec` (`run`, `state_mode`, `is_async`, `name`) has no
context in its calling convention, and `liquers-web/src/command/spec.rs` deliberately skips the
core `context` declaration convention because "a JavaScript command cannot reach the context at
all".

Re-checked at HEAD (2026-10-26):

- **Declaration half: done in core.** The Command Declaration Format removes an argument named
  `context` from `arguments` and records its position in `registration.context`
  (`reference/COMMAND_DECLARATION.md`, conventions table). Nothing is missing in core.
- **Precedent: `liquers-py`.** `lq.Context` (`liquers-py/src/context.rs`) offers `evaluate`,
  `info`, `warning`, `error` and `is_cancelled`, is inserted at the recorded position, and is
  invalidated when the callable returns (`reference/PYTHON_INTEGRATION.md`, "Context").
- **The conformance row overstates it.** `liquers-web/README.md` lists `COMMAND` complete including
  `COMMAND07`, but `command07_context_injection` (`liquers-web/tests/commands_COMMAND.rs`) asserts
  only that a command runs — it passes whatever the adapter does (guide §3, "Conformance tests that
  pass whatever the code does"). It should read `PARTIAL` until this lands.

## Problem Example

```javascript
liquers.registerCommand({
  name: "report", async: true,
  run: async (context) => {
    context.info("fetching");                  // TypeError today: context is undefined
    return await context.evaluate("hello/shout");
  },
});
await liquers.evaluate("report");
// expected: "HELLO, WORLD!", with an info entry "fetching" in the asset's log
```

## Impact

A JavaScript command can only transform its input: it cannot report progress, read an injected
value or the payload, evaluate a sub-query, or observe cancellation. It also blocks the in-command
half of `WEB-HAS-NO-TEMPLATE-COMMANDS` (rendering a template from a command).

## Expected behaviour

A JavaScript `Context` class mirroring `lq.Context`, passed at the position the core `context`
convention records (so `spec.rs` runs that convention instead of skipping it), invalidated when the
callable's promise settles. `evaluate` returns a `Promise`, so only an `async` command can await
it; a synchronous command may call `info`/`warning`/`error` and `isCancelled`. The design must keep
the adapter's rule that nothing borrowed from the environment is held while JavaScript runs, for a
re-entrant `evaluate`. `COMMAND07` is rewritten to observe a log entry and a nested evaluation.

## Related

- `COMMAND-CONTEXT-PARAM-ORDER` (closed): the Rust macro accepts `context` at any position.
- `POST-INIT-COMMAND-REGISTRATION`: same registration path, unrelated mechanism.

## Discovery

Found while classifying the macro's wrapping decisions for `COMMAND-DECLARATION-FORMAT`
(2026-08-29); re-checked 2026-10-20 and tightened 2026-10-26 (project-groups plan).
