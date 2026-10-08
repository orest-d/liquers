---
id: DEPENDENCY_CONSISTENCY_GUIDE
title: Dependency Consistency Guide
kind: guide
audience: both
area: [core/assets]
reviewed: 2026-10-08
---
# Dependency Consistency Guide

How to choose how much inconsistency an application tolerates between runs, and how to start a
trusting application from a consistent state. The model is in
[`DEPENDENCIES_STATUS.md` §Consistency policies](../reference/DEPENDENCIES_STATUS.md#consistency-policies).
This guide is the "which and how".

## The question

A stored value records what it was built from: its **direct** dependencies, with their versions.
While a process runs, the asset and dependency managers know every change made through Liquers,
and a change cascades to every dependent they hold. Between runs, anything may change: a command
is upgraded (its implementation version), a file is edited, a value is recomputed elsewhere. When
a restarted process is asked for a stored value, it can either trust it or check it, and checking
may mean reading the metadata of everything upstream.

## The two modes

| | **Trusting** — `dependency_audit: explicit` (default) | **Conservative** — `dependency_audit: on_load` |
|---|---|---|
| What a load checks | The value's own bytes, and each recorded dependency against what the managers **already** know: commands always; other values only if they were loaded or computed in this process | The same, and every recorded dependency the managers do not know is resolved from the store, **recursively** (the stored-records walk) |
| What it may serve | A value whose upstream changed between runs, until something touches that upstream or an audit runs | Nothing the stored records can show to be stale |
| Cost per load | None beyond the value itself | One metadata read per unknown upstream value, once per process |
| Typical use | Exploratory and interactive work; intermediates deleted by hand; large stores where startup must be instant | Services that must never serve a stale value |

Two further options sit between them:
- **Trusting, with a startup audit.** It costs one metadata read per stored value, once, at
  start; then it behaves like the trusting mode on a store known to be consistent.
- **Trusting, with audits on demand.** Per key or everything registered, when an operator asks.

Neither mode covers a store changed **behind a running manager's back** (another process writing
the same directory, a manual edit while the server runs). While it runs, a manager is the source
of truth. Making a running system consistent with its store again is a separate, not yet
designed operation: `ASSET-MANAGER-CANNOT-BE-SYNCHRONIZED-WITH-THE-STORE`.

## Configuring the mode

In an environment configuration document:

```yaml
assets:
  dependency_audit: on_load     # explicit (default) | on_load
```

Or with the builder:

```rust
let envref = EnvironmentBuilder::<Value>::new()
    .with_asset_manager_options(
        AssetManagerOptions::default().with_dependency_audit(DependencyAuditPolicy::OnLoad),
    )
    .with_async_store(store)
    .build()?;
```

See [`ENVIRONMENT_CONFIG.md`](../reference/ENVIRONMENT_CONFIG.md) for the other `assets` keys.
`verify_versions: on_read` (the default) is what checks a value's own bytes on load, in both
modes.

## Running a startup audit

The audit is optional. The library never runs it on its own; the application decides. Call it
once, right after the environment is built and before serving requests:

```rust
use liquers_core::assets::{AssetManager, AuditMode};
use liquers_core::query::Key;

let report = envref
    .get_asset_manager()
    .trigger_dependency_audit_store(&Key::new(), AuditMode::Expire)
    .await?;
eprintln!("startup audit: {} checked, {} expired", report.checked.len(), report.expired.len());
```

What it does:
- **Scope.** It reads the metadata of every stored `Ready`/`Override` value under the given key
  (`Key::new()` is the whole store) that has dependency records, once each.
- **Checks.** Each is checked against the current command versions and its stored upstream.
- **Stale values** are persisted `Expired`, with the reason
  `Direct { StaleDependency { dependency } }`, so the next request recomputes them.
- **Fresh values** are registered, so later loads need no further reads.

Nothing is evaluated.

- **Preview first.** `AuditMode::ReportOnly` lists `findings` (which value, broken by which
  dependency) and writes nothing.
- **Part of the store.** Pass a folder key to audit only that subtree.
- **Example.** The `liquers-axum` `basic_server` example runs it when started with
  `LIQUERS_STARTUP_AUDIT=1`.
- **Executable form.** `startup_store_audit_expires_stale_chain` and
  `startup_store_audit_report_only_changes_nothing` in
  `liquers-core/tests/dependency_audit_integration.rs`.

## Audits while running

| Call | Checks | HTTP (`liquers-axum`, admin routes) |
|---|---|---|
| `trigger_dependency_audit(query)` | The key's upstream, as far as the managers have seen it: every dependency in its upstream closure whose version they do not know | `admin/audit/{key}` |
| `trigger_dependency_audit_all_registered()` | Every unknown dependency of every value the managers hold | `admin/audit` |
| `trigger_dependency_audit_store(root, mode)` | Every stored value under `root`, from its stored records | — |

Each has an `AuditMode`: `Expire` acts, and `ReportOnly` only reports.

## Diagnosing a stale or surprising value

1. **Read its expiry reason.** It is in the value's metadata (`expiry_reason`) and the asset log:
   - `Cascaded { cause, root, via }`: `root` is where the change happened, and `via` is the
     value's own **direct** dependency through which it arrived.
   - `Direct { StaleDependency { dependency } }`: an audit found `dependency` (a command, a value,
     or a missing intermediate) no longer matching what was recorded.
2. **A value is served although its upstream changed between runs.** That is the trusting mode
   working as designed. Audit the key (`admin/audit/{key}`), run the startup audit next time, or
   switch to `on_load`.
3. **Read its records.** They list its direct dependencies only. To follow a chain, read each
   dependency's records in turn, or let an audit do it.

## Choosing, in short

- **Must never serve stale:** `on_load`.
- **Fast start, consistent start:** `explicit` plus a startup audit.
- **Interactive or exploratory, tolerant:** `explicit`, with an audit when something looks off.

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-08 | Created: the two modes, configuration, the startup audit, audits while running, diagnosis. | phase-5 (`design/dependency-chain-analysis-cost/`) |
