---
id: COMMAND-CANNOT-BE-RUN-WITH-A-RESTRICTED-CONTEXT
kind: feature
title: A command cannot be run with a restricted context
status: draft
priority: P2
complexity: M
area: [core/context, core/commands]
design: 
created: 2026-09-17
github:
---
## Problem

Every command receives a `Context`, and a `Context` is fully powered: `evaluate(&Query)` produces an
asset and may run an arbitrarily expensive recipe (`liquers-core/src/context.rs:746`),
`get_async_store()` hands out the store (`:314`), and `get_asset_manager` reaches the asset layer.
There is no way to invoke a command with less.

That is the right default. What is missing is the ability for a *caller* to say "run this command,
but it may not evaluate queries, write to the store, or otherwise cause work I did not ask for" —
and to have that enforced rather than promised.

The distinction the caller needs already exists one level up.
`CommandExecutor::execute(command_key, &State, arguments, context)`
(`liquers-core/src/commands.rs:540`) applies a command to a state and returns a value: it creates no
asset, persists nothing and cascades no dependency. So a caller can invoke a command cheaply — and
then the command's own body can reach through the context and do all three anyway.

## Impact

Any caller that wants to apply a command as a *pure function* has to trust that it is one. There is
no declaration to check and no mechanism to enforce, so the guarantee is by convention and by code
review.

The concrete case is `design/store-and-asset-search/`. Its search path applies a **projection**
command to a candidate's bytes — extracting YAML front-matter, for example — to resolve field
predicates. That is legitimate and cheap: it parses bytes the text clause is reading anyway, and
`execute` produces no asset. But a projection that quietly called `Context::evaluate` would turn one
search into an unbounded cascade of recipe evaluations across the corpus, which is exactly the
outcome that design's non-evaluation invariant exists to prevent. Today the invariant can be stated
but not enforced.

The same need appears anywhere a command is applied speculatively rather than because a user asked
for its result: validation, preview, a dry run, or a UI computing a hint.

P2: there is a workaround — declare purity as a contract and enforce it by review — and nothing is
incorrect today, because no caller yet relies on the guarantee.

## Expected behaviour

A caller can obtain a context that refuses the operations it does not want to permit, and pass that
to `CommandExecutor::execute`. A command attempting a refused operation gets an `Error`, not silent
success and not a panic.

Questions for the design:

- **Refuse or omit.** A restricted `Context` that still exposes `evaluate` but returns an error is
  simple and keeps one type; a separate, narrower context type makes the restriction visible in
  signatures but multiplies the types a command may be written against.
- **Granularity.** Probably at least: no query evaluation, no store writes, no store reads beyond the
  state already supplied. Whether these are one flag or several.
- **Declaration versus enforcement.** A `pure:` (or `restricted:`) statement in `register_command!`
  would let the registry reject a mismatch early and let a caller select commands it may run. That is
  complementary to enforcement, not a substitute.
- **Interaction with `payload_required` and `volatile`.** A command that needs a payload or is
  declared volatile is already saying something about its execution; a purity declaration should sit
  beside those rather than overlap them.
- **Async.** `execute_async` takes the same context and has the same gap.

## Discovery

Found while designing `store-and-asset-search`, 2026-09-17, working out whether a search may apply a
projection command inline without violating its non-evaluation invariant. It may — `execute` creates
no asset — but nothing stops the command's body from evaluating through its context. Verified at
HEAD: `Context::evaluate` at `context.rs:746`, `Context::get_async_store` at `:314`,
`CommandExecutor::execute` at `commands.rs:540`, and no restricted or read-only context type
anywhere in `liquers-core`.
