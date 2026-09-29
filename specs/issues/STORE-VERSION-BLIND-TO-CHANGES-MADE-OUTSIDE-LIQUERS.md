---
id: STORE-VERSION-BLIND-TO-CHANGES-MADE-OUTSIDE-LIQUERS
kind: issue
title: A stored value's version changes only when Liquers writes it, so edits made by other programs are invisible to dependency checks
status: draft
priority: P2
complexity: L
area: [core/store, core/assets]
design:
created: 2026-09-29
github:
---

## Problem

A keyed asset's **version** is a content hash that Liquers computes when *it* writes the value, and
stores in the metadata (`Version::from_bytes`, via `set_binary`, `set_state` or evaluation:
`liquers-core/src/assets.rs:2022`, `:5746`, `:5860`). Every dependency check reads that recorded
version and never the bytes: fast-track validation, cascades, and the dependency audit
(`AssetManager::version`, `assets.rs:4346`, deliberately "metadata only, never the value").

So when a program other than Liquers changes the data, nothing notices:

- **A file store directory edited by hand or by another tool.** `data/a.csv` is overwritten, and
  its sidecar `data/a.csv.__metadata__` still says version `V1`. `data/report.txt`, computed from
  `V1`, keeps being served as fresh, even by a strict audit.
- **A file dropped in without a sidecar.** It has no version at all, so an audit reads "no version"
  and expires every dependent that recorded one. That is safe, but it recomputes even when nothing
  changed.

## Impact

The dependency machinery is correct only for data that Liquers writes. For stores that other
programs also write (shared folders, object storage fed by pipelines), derived results can be
stale with no signal. `design/dependency-audit-and-expiry-provenance/` makes audits compare
versions correctly and adds a strict on-load policy, but it can only compare the versions it is
given. This issue is the reason that policy is not a guarantee for such stores.

Workaround: write through Liquers (`set_binary` / `set_state`), or remove the sidecar after an
outside edit so the audit treats the key as changed.

## Expected behaviour

A store can report a **change token** that it observes without Liquers having written the value, such
as a file's modification time and size, an object store's ETag or generation, or a database row
version. The version check compares against it, or the recorded version is refreshed from it.
Decide per backend whether this is cheap enough to do on every check or only in an audit. Keep the
"metadata kept, data deleted" workflow working (see `DEPENDENCY-AUDIT-POLICY-NOT-EXPRESSIBLE`).

## Discovery

Found 2026-09-29 while writing worked examples for `dependency-audit-and-expiry-provenance`. The
first example, "a.csv changed, so report.txt must expire", holds only if a.csv changed through
Liquers.
