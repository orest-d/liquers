---
id: WEB-COMMAND-LOOKUP-IGNORES-NAMESPACE
kind: issue
title: describeCommand and unregisterCommand take a bare name, so namespaced commands cannot be addressed
status: draft
priority: P2
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`describeCommand(name)` and `Environment.describeCommand(name)` look up
`CommandKey::new("", "", name)` only (`liquers-web/src/environment.rs`, `describe_command_on`,
`LiquersEnvironment::describe_command`), so a command registered with `namespace: "demo"` is
reported as `null`.

`unregisterCommand(name)` (`unregister_command_on`) drops **every** retained declaration whose name
is `name`, in any namespace and realm, but removes only the root-namespace key from an un-shared
registry. Measured on a wasm build (2026-10-26): after registering `{ name: "x", namespace: "demo" }`
and evaluating once, `describeCommand("x")` is `null`, `unregisterCommand("x")` returns `true`, and
`ns-demo/x` is gone.

## Impact

A page cannot inspect a namespaced command, and removing a root command also removes same-named
commands of other namespaces. Workaround: unique names across namespaces.

## Expected behaviour

Both take the command's namespace (and realm), as `liquers-py`'s `unregister(name, namespace="")`
and `command_metadata(name, namespace="")` do, and act on exactly that key.

## Discovery

Writing `specs/guides/JAVASCRIPT_GUIDE.md` and checking its examples against a build (2026-10-26).
