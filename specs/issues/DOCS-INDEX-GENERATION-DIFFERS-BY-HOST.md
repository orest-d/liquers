---
id: DOCS-INDEX-GENERATION-DIFFERS-BY-HOST
kind: issue
title: Documentation index generation differs by host filesystem ordering
status: closed
priority: P2
complexity: S
area: [docs, build]
design:
created: 2026-09-04
github:
---

## Problem

`scripts/docs_index.py` sorted `Path` objects and iterated design folders directly. Windows and
Linux therefore produced different ordering for mixed-case paths and phase files, so a committed
index generated on one host failed `--check` on the other.

## Impact

The PR documentation check reported stale generated indexes even when the only difference was
host ordering. Absolute workspace paths had also appeared in `index.md` before the merged
path-independent-link correction.

## Expected behaviour

Generated indexes must be byte-stable across supported hosts and must never include machine-local
paths.

## Discovery

Found on 2026-09-04 while verifying the merged PR #62 documentation check.

## Resolution

Closed on 2026-09-04. `docs_index.py` now sorts document and design-phase paths by a stable,
case-folded POSIX key. Its focused unit test passes, `docs_index.py --check` passes, and the
generated files contain no machine-local paths.
