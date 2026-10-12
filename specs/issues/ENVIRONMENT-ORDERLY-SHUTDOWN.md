---
id: ENVIRONMENT-ORDERLY-SHUTDOWN
kind: feature
title: An environment cannot be shut down in an orderly way
status: draft
priority: P3
complexity: M
area: [core/context, core/assets]
design: 
created: 2026-10-12
github:
---
## Problem

There is no `Environment`-level way to stop an environment in an orderly way. The only stop today
is `DefaultAssetManager::shutdown` (`liquers-core/src/assets.rs`), which stops the job queue and
the expiration monitor; nothing stops the environment as a whole, waits for or cancels its
in-flight work, or tells services built on it that it is going away. Once
`environment-manager-reference-cycle` lands, dropping the last `EnvRef` frees the environment, but
`Drop` cannot `await`, so a service that needs asynchronous cleanup (flushing, closing a
connection) has no hook. No such service exists today.

## Problem Example

```rust
let envref = builder.build().await?;
let asset = envref.evaluate("-R/data/report.txt").await?;   // still running
// Soft reboot: today there is no call that stops new work, settles `asset` and runs cleanup,
// then reports when it is done.
envref.shutdown().await?;    // wanted
```

## Impact

None today; a soft reboot, a per-tenant environment or a page reload in `liquers-web` would want
it. Workaround: drop the environment (after the reference-cycle fix) and accept that in-flight work
keeps it alive until it finishes.

## Expected behaviour

An `async` shutdown on the environment whose contract settles, at least:

- in-flight work: wait for it, cancel it, or a choice (`asset-cancellation-outcome` owns what a
  cancelled asset reports);
- after shutdown: every call fails with a typed error rather than panicking, through
  `try_get_envref` from `environment-manager-reference-cycle`;
- idempotence: a second call is a no-op;
- a registration point for services that need asynchronous cleanup, run in a defined order;
- what `liquers-web` and `liquers-py` expose for it.

## Discovery

Split off on 2026-10-12 from `environment-manager-reference-cycle` (its Phase 1 open question 4),
by Orest's decision: "shutdown sounds potentially useful - there might be services needing a
shutdown (though there are none today)". Requires that design (weak `EnvRef` back-references and
`try_get_envref`).
