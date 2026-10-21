---
id: CORE-VALUE-INTERFACE-CAPABILITY-SPLIT
kind: issue
title: `ValueInterface` bundles capabilities every implementor must provide
status: accepted
priority: P2
complexity: L
area: [core/value, lib/value, py]
design: value-interface-capability-split
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/value.rs:40` — `// TODO: Remove the serialization and deserialization from
ValueInterface`. Naming is also unsettled: `:192` and `:197` propose renaming `identifier` and
`type_name` to `type_identifier` and `detailed_type_identifier`.

Every value type must implement the whole surface whether or not the capability makes sense for it.

## Problem Example

`polars.DataFrame`'s defaults are written in its `TypeInfo` (`liquers-lib/src/value/mod.rs`
`type_descriptions`) and again in four `match` arms (`type_name`, `default_extension`,
`default_filename`, `default_media_type`), plus four delegations in `CombinedValue`. Changing the
`TypeInfo` alone leaves `default_extension()` stale. Should be: one description, every default
derived from it.

## Impact

Implementing a value type is more work than it needs to be, and the trait cannot grow a capability
without breaking every implementor — including `liquers-py` and `liquers-web`.

## Expected behaviour

A small core trait plus capability traits (serialization, description, conversion) that a value
type opts into. WP-11 proposes this and notes it should follow the metadata-consistency work.

Wants a design: it is a breaking change reaching both bindings.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #9, work package WP-11. Verified against HEAD: markers present at `value.rs:40`, `:192`, `:197`. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Triage 2026-10-20 (bulk design)

Persists, partly re-scoped. The markers moved: `liquers-core/src/value.rs` trait-head TODO (now at
the `ValueInterface` definition) and the rename TODOs on `identifier` / `type_name`. Two claims are
narrower at HEAD: `liquers-py` no longer implements `ValueInterface` (commit `0d0a4c1`; it
implements `ForeignValue` only), and new capabilities *have* been added without breaking
implementors (`data_node`, `type_descriptions`, by default methods). What remains is that a value's
identity and description are declared in three parallel traits (`ValueInterface`, `ValueExtension`,
`ForeignValue`) and duplicated against `TypeInfo`. Design: `specs/design/value-interface-capability-split/`.
