---
id: PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO
kind: issue
title: Plan dependency records are written with a hard-coded zero version although real versions are available
status: draft
priority: P2
complexity: S
area: [core/plan, core/assets]
design: keyed-expiry-cascade-fix
created: 2026-09-05
github:
---

## Problem

`finalize_plan` (`liquers-core/src/interpreter.rs:71`) records every static plan dependency with a
literal zero:

```rust
for plan_dep in &plan.dependencies {
    context
        .add_dependency(DependencyRecord::new(plan_dep.key.clone(), Version::new(0)))
        .await;
}
```

Most of those dependencies are command metadata and implementation keys
(`ns-dep/command_metadata-…`, `ns-dep/command_impl-…`), and the dependency manager **already holds
real versions for them**: `AssetManager::start` registers every command's declared versions through
`load_command_versions_sync` (`assets.rs:3419`) before any asset is evaluated. The very next block
of the same function proves the versions are reachable — `register_plan_dependencies`
(`assets.rs:3918`) looks each one up with `get_version` and registers the graph edge with the real
value.

So the graph edge carries a concrete version and the persisted record carries zero, from two
statements a few lines apart.

Measured: every command dependency in a persisted record reads
`00000000000000000000000000000000`, including `ns-dep/command_impl---world` for a command declared
with `version: 2`.

## Impact

A stored asset cannot be invalidated by a command version change across a restart. `try_fast_track`
compares its recorded dependency versions against the manager's (`assets.rs:1119`), and a zero
record matches anything (`Version::matches`, `metadata.rs:65`), so an asset produced by an older
implementation of a command is reloaded and served as fresh.

In-process this is covered: `refresh_command_versions_and_expire` cascades on a changed command
version through the graph edges, which do carry real versions. It is only the persisted path that
loses the information.

P2: it needs a restart plus a command version change to observe, the in-process path works, and
`register_plan_dependencies` already gives the graph the right value. S because the fix is to read
the version that is already being looked up a few lines below.

## Expected behaviour

The record carries the same version `register_plan_dependencies` registers — i.e. look the key up
in the dependency manager once and use it for both, falling back to `Version::unknown()` only when
the manager genuinely has none (a command that declared no version is never registered at all,
since `load_command_versions_sync` skips `is_unknown()`).

Before changing it, note the consequence: real command versions in records mean that after any
command's declared version changes, every persisted asset depending on it is invalidated on first
load. That is the intended behaviour of a version, but it is a behaviour nothing exhibits today, so
it should be turned on deliberately rather than as a side effect.

## Discovery

Found on 2026-09-05 during the final cross-document review of `keyed-expiry-cascade-fix`, whose
"command keys are special because the manager's knowledge of them is complete" argument turned out
to be unreachable in production: no production caller ever passes a concrete command version to
`DependencyManager::add_dependency`. Independent of, but symptomatic with,
`DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES`.
