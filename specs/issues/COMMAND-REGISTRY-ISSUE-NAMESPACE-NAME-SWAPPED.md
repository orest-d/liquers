---
id: COMMAND-REGISTRY-ISSUE-NAMESPACE-NAME-SWAPPED
kind: issue
title: CommandRegistryIssue constructors transpose namespace and name
status: closed
priority: P3
complexity: S
area: [core/commands]
design: command-registry-issue-fields-coverage
created: 2026-08-12
github:
---
## Problem

`CommandRegistryIssue::warning` and `CommandRegistryIssue::error`
(`liquers-core/src/command_metadata.rs:36-41`) both pass their arguments to `new` in the wrong
order, swapping `namespace` and `name`:

```rust
pub fn new(realm: &str, namespace: &str, name: &str, is_error: bool, message: String) -> Self

pub fn warning(realm: &str, namespace: &str, name: &str, message: String) -> Self {
    CommandRegistryIssue::new(realm, name, namespace, false, message)
    //                               ^^^^  ^^^^^^^^^ transposed
}
pub fn error(realm: &str, namespace: &str, name: &str, message: String) -> Self {
    CommandRegistryIssue::new(realm, name, namespace, true, message)
}
```

Every issue built through either helper therefore reports the command name in its `namespace` field
and the namespace in its `name` field. `CommandMetadata::check()` (`:946`) builds all of its issues
this way, so a genuine problem — an empty command name, the reserved name `ns` — would be attributed
to the wrong command.

## Impact

Currently latent. `CommandMetadata::check()` has no caller anywhere in the workspace and neither
helper is used outside its own definition, so no transposed issue reaches a user today. The defect
matters as soon as anything consumes `check()`, which `VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`
proposes.

## Expected behaviour

Both helpers forward `realm, namespace, name` in declaration order.

## Fix direction

Correct the two call sites and add a unit test asserting that
`CommandRegistryIssue::error("r", "ns", "cmd", …)` yields `namespace == "ns"` and `name == "cmd"` —
the argument lists are three same-typed `&str` parameters, which is what let the transposition
through unnoticed and what will let it back in without a test.

## Discovery

Found while locating the registration-time home for the variadic-argument check in
`VARIADIC-ARGUMENT-STARVES-LATER-ARGUMENTS`.

## Resolution

Closed on 2026-09-01. Both convenience constructors now preserve `(realm, namespace, name)` in
declaration order, with direct warning/error tests and a reserved-name diagnostic regression test.
