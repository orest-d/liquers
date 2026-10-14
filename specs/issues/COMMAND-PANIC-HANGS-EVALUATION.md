---
id: COMMAND-PANIC-HANGS-EVALUATION
kind: issue
title: A command that panics leaves its asset unfinished, so evaluation waits forever instead of failing
status: draft
priority: P1
complexity: M
area: [core/assets]
design:
created: 2026-10-13
github:
---
## Problem

When a command function panics during evaluation, the asset that was evaluating it is never moved
to a terminal state (`Error`). Whoever awaits the asset (`EnvRef::evaluate(..).get()`, a dependent
asset) waits forever: no error is returned and nothing times out.

Observed with `ns-pl/sort` panicking inside polars on a current-thread runtime
(`POLARS-COLLECT-PANICS-ON-CURRENT-THREAD-RUNTIME`): the asset log stopped at
`Asset … status changed from Processing to Submitted` / `Received message: JobSubmitted`, and the
test that awaited it ran past 60 seconds until it was killed. The job runs in a spawned task
(`liquers-core/src/assets.rs`); a panic there ends the task with a `JoinError` that does not
appear to be turned into an asset error. The exact site was not traced.

## Impact

A single panicking command (a bug in a command, a panicking dependency, a `unwrap` in library
code — `LIBRARY-CODE-USES-UNWRAP-AND-EXPECT`) turns into a hang of every request that needs that
asset, in the server as in tests. A hang is harder to diagnose than an error, and it can tie up a
request indefinitely. No workaround beyond not panicking.

## Expected behaviour

A panic in a command becomes an `Error` on the asset (the panic message as the error text), so the
evaluation fails promptly and dependants see the failure. Options: catch the panic around the
command call (`std::panic::catch_unwind` / `FutureExt::catch_unwind`), or handle the spawned
task's `JoinError` by setting the asset's status. Which one fits the inline (spawn-free) manager
too is a design question.

## Discovery

2026-10-13, implementing `record-filter-commands` Step 10: tests evaluating `ns-pl` commands
under `#[tokio::test]` hung after the polars panic instead of failing.
