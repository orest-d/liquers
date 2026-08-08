---
id: POST-INIT-COMMAND-REGISTRATION
kind: issue
title: Registering a command after Environment::to_ref requires a rebuild
status: draft
priority: P3
complexity: M
area: [core/commands, web]
design: 
created: 2026-08-08
github:
---
**Downgraded.** This was first filed as a P1 blocker on the claim that a command cannot be
registered after an environment is shared. That claim was wrong: the ordinary Rust ordering —
build the registry, then the environment, then `to_ref` — applies to a language integration too,
and can simply be done again. `liquers-web` implements it and has no such limitation.

## Problem

Registration needs `&mut CommandRegistry`, and there is no path to one once
`Environment::to_ref` has run:

- `Environment::to_ref(self)` **consumes** the environment into `EnvRef(Arc<E>)`
  (`liquers-core/src/context.rs:213`). `Arc::get_mut` does not help — `init_with_envref` stores a
  clone of the `EnvRef` inside the environment, so the strong count is never 1.
- `Environment::get_command_executor(&self) -> &Self::CommandExecutor`
  (`liquers-core/src/context.rs:170`) returns a **reference**, so an implementor cannot place the
  executor behind a `RefCell`/`RwLock` either.

So an environment is effectively frozen once shared.

## How this is handled today

`liquers-web` keeps the environment un-shared and mutable until the first evaluation, and rebuilds
it when a command is registered afterwards: a fresh environment is constructed, every retained
declaration is replayed into it along with the new one, and the shared handle is swapped. Replay
goes through the same registration path as the original, so the two cannot drift.

The cost of a rebuild is the **asset cache**, discarded with the old environment. An evaluation
already in flight keeps the old `EnvRef` and completes against it, so nothing is interrupted — it
simply does not observe the new command. Registering all commands before the first evaluation
avoids rebuilding entirely, which is the ordinary flow.

## Why it is still worth fixing

The rebuild is correct but wasteful, and it gets more wasteful as an environment accumulates state
that is expensive to rebuild — most obviously a populated asset cache, and later a configured store
(`STORE`) or recipe provider (`RECIPE`). An application that registers commands per route pays it
on every route change.

## Expected behavior

Registering a command should not require discarding the environment. Two candidate designs:

1. **Interior mutability in `CommandRegistry`.** Hold the executor maps and the metadata registry
   in `RefCell` (wasm) / `RwLock` (native, selected by the existing `MaybeSend`/`MaybeSync` split),
   and add `&self` registration methods alongside the current `&mut self` ones.
   `get_command_executor` keeps returning `&CommandRegistry`, so the trait is unchanged and the
   addition is backward compatible. **Recommended** — additive and localized.
2. **A registration hook on `Environment`**, e.g. `fn register_command(&self, …)`. Larger surface,
   and every implementor must decide what to do.

Either way the concurrency story needs stating: registration mutates a structure evaluation reads,
so on native the lock discipline matters, and on wasm the `RefCell` must not be held across an
`await`.

## Discovery

Found while implementing `specs/liquers-web` milestone M4, and corrected in the same milestone once
the rebuild approach was identified. Recorded because the underlying constraint is real and worth
removing, and because the first diagnosis being wrong is itself worth remembering: the absence of a
mutable path is not the same as the absence of a way forward.
