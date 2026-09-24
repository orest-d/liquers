---
id: METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED
kind: issue
title: A value stored as metadata only is reloaded through the corrupted-data path
status: draft
priority: P3
complexity: S
area: [core/assets]
design:
created: 2026-09-24
github:
---
# A value stored as metadata only is reloaded through the corrupted-data path

## Problem

When a keyed asset's value has no byte form, the produce path stores its metadata only —
`assets.rs`, step 8 of `set_state`: "Non-serializable data – store metadata only", via
`store.set_metadata`. The status in that metadata is the value's own, typically `Ready`.

When the key is next requested, the fast-track load (`assets.rs:1103-1150`) sees a stored entry
with a `Ready` status, calls `store.get`, and hands the **empty** bytes to
`deserialize_stored_value`. That fails, and the failure is logged as

```
Asset … fast-track failed to deserialize stored value (treated as corrupted): …
```

before the asset falls back to evaluating its recipe. The outcome is right — the value is
re-derived — but it is reached through the branch meant for damaged data, and nothing in the stored
metadata says the absence of bytes was intended.

## Impact

- **Diagnostics lie.** Every reload of such a value prints a corruption message, so a real
  corruption is indistinguishable from the normal case in the logs.
- **Wasted work**: a store read and a deserialization attempt that cannot succeed, on every load.
- **More values will take this path.** UI elements, egui widgets and foreign handles do today. The
  record design adds every non-manifest `RecordSource` (`specs/design/record-streams/` Phase 2,
  "A source serializes only as its manifest"), and the type registry cannot flag those in advance,
  because `RecordSource` as a type *does* have a byte form — the manifest — while most instances do
  not.

## Expected behaviour

A metadata-only entry is recognizable as intended — for example a marker in the metadata written by
the step-8 branch, or a status meaning "no bytes by design" — and the fast-track load skips the read
and the deserialization for it, going straight to recipe evaluation without logging corruption.
Since the decision is per instance rather than per type, the marker has to live in the stored
metadata rather than in `TypeInfo`.

## Discovery

Found 2026-09-24 while checking the `record-streams` claim that a refused source "is stored as
metadata only and re-derived from its recipe". The claim holds, through this path.
