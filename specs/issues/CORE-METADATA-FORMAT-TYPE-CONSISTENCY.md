---
id: CORE-METADATA-FORMAT-TYPE-CONSISTENCY
kind: issue
title: Metadata data format and type can disagree with the value
status: draft
priority: P1
complexity: M
area: [core/value]
design: metadata-consistency
created: 2026-08-08
github:
---
## Problem

Nothing enforces that a `State`'s metadata `data_format` and type identifier match the value it
describes. They are set independently and can drift apart.

## Impact

Silent corruption: a value serialized under a format its metadata does not name deserializes as the
wrong type, or fails far from the cause. `specs/design/metadata-consistency/FINDINGS.md` catalogues the
candidate invariants.

## Expected behaviour

The invariants are stated and checked at the points metadata is set, with `debug_assert!` at least
and a typed error where a caller can act on it.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-4, with `specs/design/metadata-consistency/`. Verified against HEAD: no counterpart in the TODO audit; the findings document stands. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
