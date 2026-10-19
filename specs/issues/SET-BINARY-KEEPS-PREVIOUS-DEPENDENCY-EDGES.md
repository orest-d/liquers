---
id: SET-BINARY-KEEPS-PREVIOUS-DEPENDENCY-EDGES
kind: issue
title: A set_binary that replaces a keyed asset keeps the incoming dependency edges of the value it replaced
status: draft
priority: P3
complexity: S
area: [core/assets]
created: 2026-10-18
github:
---

## Problem

Since `design/replacing-write-drops-stale-dependency-edges/`, `set_state` (both built-in managers
and `AssetManager::track_installed`) replaces a key's incoming dependency edges with the records in
the new metadata. `set_binary` does not: it publishes the new version but neither clears the key's
incoming edges nor loads the supplied metadata's dependency records. A key that was computed (or
installed by `set_state`) with dependency `d`, then overwritten by `set_binary`, is still expired by
a later write of `d`, although its bytes no longer come from `d`. The effect is a spurious expiry,
not a stale value; and the supplied records are not registered, so a real dependency named there
does not expire the key.

## Expected

`set_binary` treats its metadata's dependency records as `set_state` does: clear the key's incoming
edges, then load the records (`DependencyManager::track_replacing_asset` needs an asset; the binary
path has none, so a key-based variant or an equivalent sequence is needed).

## Notes

Left out of `replacing-write-drops-stale-dependency-edges` as a non-goal.
