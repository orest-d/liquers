---
id: CORE-METADATA-FORMAT-TYPE-CONSISTENCY
kind: issue
title: Metadata data format and type can disagree with the value
status: closed
priority: P0
complexity: M
area: [core/value]
design: value-type-system
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

## Resolution

Fixed 2026-08-18 by the `value-type-system` design. The invariants are stated in
`specs/reference/VALUE_TYPE_SYSTEM.md` and enforced at `AssetManager::set_binary`/`set_state` in two
tiers: hard rejections for what makes a stored value unreadable — an unregistered identifier, a data
format the type cannot be written in, a malformed media-type override — and soft `LogEntry`
warnings for legitimate divergence such as an explicit media-type override.

Rather than only validating the pair, the design gave the identifier a meaning it did not have.
`Value::I32.identifier()` was `"generic"` while the deserializer branched on `"i32"`, so five
variants shared one answer and an integer written as text read back as text. Identifiers are now
bare CamelCase names matching what the deserializer dispatches on, and every type declares which
formats it can be written in.

Backward compatibility was deliberately not preserved, at the user's direction: stored identifiers
changed and no migration is provided; data written by an older build degrades on read.

The partial-document question this issue was handed by
`CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON` is also answered: `MetadataRecord` gained
container-level `#[serde(default)]`, so a partial document deserializes into a record instead of
falling through to the legacy branch.

See `specs/design/value-type-system/phase5-documentation.md`.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-4, with `specs/design/metadata-consistency/`. Verified against HEAD: no counterpart in the TODO audit; the findings document stands. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.
