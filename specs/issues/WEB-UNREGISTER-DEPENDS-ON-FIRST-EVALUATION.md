---
id: WEB-UNREGISTER-DEPENDS-ON-FIRST-EVALUATION
kind: issue
title: unregisterCommand removes a built-in command before the first evaluation but not after it
status: draft
priority: P3
complexity: S
area: [web]
design:
created: 2026-10-26
github:
---
## Problem

`unregister_command_on` (`liquers-web/src/environment.rs`) has two paths. While the environment is
un-shared it calls `CommandRegistry::unregister` on the live registry, which removes whatever is
registered under the key, a built-in Rust command included. Once shared, it only drops retained
JavaScript declarations and rebuilds, so a built-in is untouched and the call returns `false`.
Measured on a wasm build (2026-10-26): `unregisterCommand("to_metadata")` straight after `init()`
returns `true` and `describeCommand("to_metadata")` is then `null`; the same call after an
evaluation returns `false` and the command stays.

The same split decides what unregistering a JavaScript replacement of a built-in does: after the
first evaluation the rebuild restores the built-in; before it, the key is removed outright and the
built-in returns only at the next rebuild.

## Impact

The outcome of one call depends on whether anything has been evaluated yet, which a page does not
usually track. Low in practice: removing a built-in is rare.

## Expected behaviour

One rule for both paths, as `liquers-py` has: unregistering removes a JavaScript command (restoring
a built-in it replaced), and removing a built-in is either always allowed and recorded so a rebuild
keeps it removed, or always refused.

## Discovery

Writing `specs/guides/JAVASCRIPT_GUIDE.md` and checking its examples against a build (2026-10-26).
