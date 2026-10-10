---
title: Command Design Guide
kind: guide
audience: internal
area: [core/commands]
reviewed: 2026-10-10
---
# Command Design Guide

How a command should behave once it is registered: what it can count on while it runs and what it
owes the asset it produces. Registration itself — the `register_command!` DSL, metadata, generic
environments — is in [`COMMAND_REGISTRATION_GUIDE.md`](COMMAND_REGISTRATION_GUIDE.md).

It covers cooperative cancellation (design: `specs/design/asset-cancellation-outcome/`) and when a
command should be an alias of another rather than an implementation of its own (design:
`specs/design/command-alias-contract/`).

## Cooperative cancellation

### What `cancel()` guarantees

`AssetRef::cancel()` (also `POST q/cancel`, `POST key/cancel` over HTTP) is a **request**, and it
is best-effort:

| Asset status when cancelled | Outcome |
|---|---|
| `Submitted` (queued, not started) | Ends `Cancelled` at once; the command is never invoked |
| `Processing` / `Partial`, async command suspended at an `.await` | The evaluation is dropped at that point; ends `Cancelled` |
| `Processing`, command still running synchronous code | Nothing can interrupt it. If it checks the request (below) it can stop; if it returns `Ok`, the asset ends ready and **is stored** as if no cancel had happened |
| `Dependencies` (waiting for a dependency) | This asset ends `Cancelled`; its dependencies carry on — they may be shared with other dependents |
| Already finished, `None` or `Recipe` | No change |

`cancel()` returns `Ok(())` in every case, whether or not it took effect, after waiting at most five
seconds for the run to finish (native; on wasm it returns immediately). Read the status afterwards
to learn the outcome. Subscribers see exactly one terminal status per run.

A completed run wins over a late cancel by design: cancelling exists to save work, not to throw
away work that is already done.

### Checking for cancellation in a command

Take `context` and check between units of work:

```rust
fn process_rows(state: &State<Value>, context: Context<E>) -> Result<Value, Error> {
    let mut out = String::new();
    for line in state.try_into_string()?.lines() {
        context.check_cancelled()?;      // Err(cause) once a cancel was requested
        out.push_str(&expensive(line));
    }
    Ok(Value::from(out))
}
register_command!(cr, fn process_rows(state, context) -> result)?;
```

- `context.is_cancelled() -> bool` — lock-free, cheap enough to call in a tight loop, from sync and
  async commands alike.
- `context.check_cancelled() -> Result<(), Error>` — `Err` with the request's cause, an
  `ErrorType::Cancelled` error whose `query` names the asset that was cancelled.

Check where the command could otherwise run for a long time: per row, per file, per request.

### Sync versus async commands

An **async** command is cancelled at its next `.await` without doing anything: the run stops polling
it. Code after that `.await` does not run, so do not leave external state half-written across an
`.await` that a cancel can land on.

A **sync** command blocks its worker thread for its entire run and cannot be stopped from outside.
It must check `is_cancelled()` itself or it will run to completion. A long sync command that never
checks makes `cancel()` time out after five seconds (it still returns `Ok`).

Cancellation takes effect at the next genuine suspension point of the whole evaluation. Once the last
step of the plan has returned and nothing else awaits, the result is installed and stored; this is
not cancellable.

### Returning a cancellation

A command that returns an error with `ErrorType::Cancelled` ends its asset `Cancelled`, not `Error`,
whether or not a cancel was requested, and nothing is stored. Return the error from
`check_cancelled()` unchanged; to give up on your own, return `Error::cancelled("why")` — the asset's
key (or query) is filled into `query` when the error names none.

**Wrapping a cancellation in another error type turns it into a failure.** `?` propagates it
unchanged; `Error::from_error(ErrorType::General, e)` or a `format!` of it does not.

The cancelled asset records the cancellation in its metadata (`error_data`, with `is_error` false);
`State::value_error()` and asset info return it.

### Dependencies and cascade

A dependency waited for through the context (`context.get_dependency_state`,
`context.wait_for_dependency`) or by the recipe that ends `Cancelled` makes the wait return that
dependency's cancellation error, **unchanged**: its `query` still names the asset whose cancel was
requested, however deep the chain. Propagate it with `?` and this asset ends `Cancelled` too, with
one warning in its log: `Dependency <dep> was cancelled; root cause: <root>`.

To tolerate a cancelled dependency, handle it and return `Ok`; the asset then ends ready:

```rust
match context.get_dependency_state(&query).await {
    Ok(state) => Ok(Value::from(state.try_into_string()?)),
    Err(e) if e.is_cancelled() => Ok(Value::from("fallback")),
    Err(e) => Err(e),
}
```

A cascade runs upward only. Cancelling an asset never cancels its dependencies, and a shared
dependency's cancel reaches every asset waiting for it, including other clients'. When an asset's own
cancel was requested and a dependency is cancelled too, its own cancel is recorded as the cause and
no cascade warning is logged.

### Testing a cancellable command

A sync command blocks its worker thread, so a test that cancels it needs a multi-threaded runtime:
`#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`. Wait for `Status::Processing`, call
`cancel()`, then assert the terminal status. See `liquers-core/tests/asset_cancellation.rs`.

## Aliases

An **alias** is a command without an implementation: a name and an argument interface bound to
another command — its target — with some of the target's leading arguments fixed. `ns-pl/head-10`
runs `pl/slice` with `offset = 0, length = 10`. How to register one is in
[`COMMAND_REGISTRATION_GUIDE.md`](COMMAND_REGISTRATION_GUIDE.md) §2 *Registering an alias*; the
exact contract is [`reference/COMMAND_ALIASES.md`](../reference/COMMAND_ALIASES.md).

### When to design a command as an alias

- **A convenience wrapper.** A general command plus a common, fixed choice of its leading arguments
  deserves its own name. `pl/head(n = 5)` is `pl/slice(offset = 0, length = n)`: one implementation,
  two names, and no chance of the two drifting apart. Write the general command first; add the
  convenient names as aliases of it.
- **A bridge.** Many declared commands are executed by one generic executor, and the head
  parameters say *which* thing to run. liquers-py declares each Python function as an alias of
  `pycall` whose heads are the module, the function and how to pass the state. The declared
  command keeps its own name and arguments; the bridge stays one registered implementation.

### When not to

- **The behaviour differs, not only the arguments.** `rec/head` materializes its rows into a batch;
  `rec/slice` returns a view over the source. That is a different command, so `rec/head` is
  implemented, not aliased.
- **The target's arguments would need reordering, dropping or computing.** An alias passes the
  heads, then its own arguments, in order. Argument mapping is a later design; until then, write a
  small command.
- **The target is itself an alias.** Aliases do not chain; point the new alias at the implementing
  command.

### Consequences to design for

- **The alias's arguments are its public interface.** Their names, labels and defaults are what
  users and recipes see (`n`, not `length`); recipe overrides use those names.
- **Behaviour flags come from the target.** Volatility, payload requirement and expiration of the
  target always apply; an alias cannot hide them.
- **Errors name both.** A failure reports the target and adds *"(via alias 'pl/head')"*; the plan
  records the alias as the action's `origin`.
- **Caching follows the alias.** Results depend on the alias's metadata as well as the target's, so
  changing the alias's head or arguments recomputes them. Turning an existing command into an alias
  recomputes its cached results once.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-10 | New §Aliases: what an alias is, when to use one (convenience wrapper, bridge), when not to, and the design consequences. Introduction no longer limited to cancellation. | phase-5, `design/command-alias-contract/` |
| 2026-10-09 | Created: cooperative cancellation — what `cancel()` guarantees, `is_cancelled` / `check_cancelled`, sync vs async commands, returning and propagating `Error::cancelled`, cascade through dependencies, testing. | phase-5, `design/asset-cancellation-outcome/` |
