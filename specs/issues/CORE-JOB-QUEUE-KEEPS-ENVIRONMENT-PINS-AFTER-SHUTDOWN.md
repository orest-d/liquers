---
id: CORE-JOB-QUEUE-KEEPS-ENVIRONMENT-PINS-AFTER-SHUTDOWN
kind: issue
title: A job still running at shutdown keeps its environment pin in the job queue forever
status: closed
priority: P2
complexity: S
area: [core/assets]
design:
created: 2026-10-27
github:
---
## Problem

`JobQueue.jobs` holds an `EnvRef` pin beside each parked or running job
(`liquers-core/src/assets.rs`, `struct JobQueue`). The pins are released only by
`cleanup_completed_internal`, which runs inside `JobQueue::run`'s loop. After `shutdown()` the loop
returns at its first check, without a final cleanup.

So any job that is still running at shutdown, or that finished after the loop's last cleanup pass,
keeps its pin. The pin holds the environment, the environment owns the asset manager, and the
manager owns the queue holding the pin: the environment is never freed. This is the cycle that
`ENVIRONMENT-MANAGER-REFERENCE-CYCLE` removed, coming back through the shutdown path.

## Impact

Whether an environment shut down with work in flight is freed depends on timing. `liquers-py`'s
`test_RUNTIME05_built_environment_releases_handles_on_shutdown` cancels a running command and shuts
down; about 1 run in 15 under load the environment's Python callables are never released (observed
on CPython 3.13 and 3.14t on 2026-10-27, `design/liquers-py-async/` Phase 4). The callables and the
whole environment, store and cache included, leak until the process exits.

## Expected behaviour

After `shutdown()` the queue holds no pins: a job still running keeps its environment through its
own context until it finishes, and a parked job, which will never start, keeps nothing. A
likely fix: when `run` sees the shutdown flag, take the whole `jobs` list out under the lock and
drop it after the lock is released, as `cleanup_completed_internal` does for finished jobs.

## Resolution (2026-10-27)

Fixed in `liquers-core/src/assets.rs`: when `JobQueue::run` sees the shutdown flag it drops every
pin (`release_pins`), and `pin` adds none after shutdown, so a job re-parked or started later
cannot leave one behind. A running job keeps its environment through its own claim until it
finishes. Evidence: `teardown12_job_running_at_shutdown_releases_its_environment` and
`teardown12_job_parked_at_shutdown_releases_its_environment` in
`liquers-core/tests/environment_teardown.rs`, both failing before the fix.
