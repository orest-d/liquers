---
id: WEB-COMMAND-NAME-WITH-A-DASH-IS-ACCEPTED
kind: issue
title: registerCommand accepts a command name that no query can spell
status: draft
priority: P3
complexity: S
area: [web, core/commands]
design:
created: 2026-11-10
github:
---

## Problem

`liquers.registerCommand({ name: "a-b", run: () => 1 })` succeeds. A query cannot call the command:
in `a-b`, the dash separates the action `a` from its argument `b`. The same holds for a name with a
dot, which a query reads as a file name. The reserved-name check (`ns`, and the `web` namespace)
runs, the character check does not.

## Impact

A mistake in a declaration is reported much later, as `action_not_registered` for a different
name. Low: the Web user guide states the naming rule.

## Expected behaviour

A declaration whose name is not a valid action name is refused in the registration report, in
every language that registers from metadata.

## Discovery

A probe while writing chapter 5 of the Web user guide (2026-11-10).
