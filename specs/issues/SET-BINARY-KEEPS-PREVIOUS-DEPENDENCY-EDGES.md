---
id: SET-BINARY-KEEPS-PREVIOUS-DEPENDENCY-EDGES
kind: issue
title: A set_binary that replaces a keyed asset keeps the incoming dependency edges of the value it replaced
status: draft
priority: P3
complexity: S
area: [core/assets]
design: set-binary-replaces-dependency-edges
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

## Problem Example

`dash.txt` was installed by `set_state` with a record of `data/d.txt`. Then `set_binary(dash.txt,
b"from e", metadata recording data/e.txt)`. A later write of `d` marks `dash.txt` expired (it
should not), and a later write of `e` leaves it alone (it should expire it, as it would after a
restart, when the stored records are loaded).

## Expected

`set_binary` treats its metadata's dependency records as `set_state` does: clear the key's incoming
edges, then load the records (`DependencyManager::track_replacing_asset` needs an asset; the binary
path has none, so a key-based variant or an equivalent sequence is needed).

## Notes

Left out of `replacing-write-drops-stale-dependency-edges` as a non-goal.

## Triage (2026-10-20)

Reproduces at HEAD 18732dd by reading: both built-in `set_binary` bodies call only
`publish_version`. Designed in `design/set-binary-replaces-dependency-edges/`. This issue overlaps
`EVALUATION-KEEPS-DROPPED-DEPENDENCY-EDGES` only weakly (the same invariant on another path), so
the two are designed separately.
