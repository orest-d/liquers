---
id: DOCS-STORE-CONFIG-DESCRIBES-ASYNC-FILESTORE-AS-FUTURE-WORK
kind: issue
title: Store configuration reference describes AsyncFileStore as future work
status: closed
priority: P2
complexity: S
area: [docs, core/store]
design: documentation-currentness-small-fixes
created: 2026-09-04
github:
---

## Problem

`specs/reference/STORE_CONFIG_FSD.md` describes the built-in filesystem store as `FileStore` and
says that a proper `AsyncFileStore` should be implemented. `liquers-core` already provides
`AsyncFileStore`, which implements the current `AsyncStore` contract.

## Impact

Readers configuring a filesystem store are told that the native async implementation is missing,
which contradicts the current source and `STORE_SEMANTICS.md`.

## Expected behaviour

Describe the configured filesystem implementation as `AsyncFileStore` and remove the obsolete
future-work statement, while preserving the configuration schema.

## Discovery

Found on 2026-09-04 while fixing `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS`. It is adjacent to,
but independent from, that issue's approved memory-store scope.

## Resolution

Closed on 2026-09-04. `STORE_CONFIG_FSD.md` now identifies `AsyncFileStore` as the built-in
filesystem implementation and no longer describes it as future work. The configuration schema is
unchanged; focused text validation and the documentation-index check passed.
