---
id: CORE-VALUE-INTERFACE-CAPABILITY-SPLIT
kind: issue
title: `ValueInterface` bundles capabilities every implementor must provide
status: draft
priority: P2
complexity: L
area: [core/value, lib/value, py]
design: 
created: 2026-08-08
github:
---
## Problem

`liquers-core/src/value.rs:40` — `// TODO: Remove the serialization and deserialization from
ValueInterface`. Naming is also unsettled: `:192` and `:197` propose renaming `identifier` and
`type_name` to `type_identifier` and `detailed_type_identifier`.

Every value type must implement the whole surface whether or not the capability makes sense for it.

## Impact

Implementing a value type is more work than it needs to be, and the trait cannot grow a capability
without breaking every implementor — including `liquers-py` and `liquers-web`.

## Expected behaviour

A small core trait plus capability traits (serialization, description, conversion) that a value
type opts into. WP-11 proposes this and notes it should follow the metadata-consistency work.

Wants a design: it is a breaking change reaching both bindings.

## Discovery

Migration triage, 2026-08-08. Source: `todo20260219.md` #9, work package WP-11. Verified against HEAD: markers present at `value.rs:40`, `:192`, `:197`. See `specs/DOCS_MIGRATION_PLAN.md` §4.0c.
