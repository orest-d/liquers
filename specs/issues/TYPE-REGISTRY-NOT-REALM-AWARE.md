---
id: TYPE-REGISTRY-NOT-REALM-AWARE
kind: feature
title: A query spanning two realms cannot know which types the other realm supports
status: draft
priority: P2
complexity: L
area: [core/value, core/commands, core/plan, web, axum]
design:
created: 2026-08-18
github:
---
## Problem

A query will eventually execute across more than one realm — a `wasm` frontend in the browser and
a native `axum` backend being the motivating pair. **Those realms do not support the same set of
types.** A Polars dataframe exists on the backend and not in the browser; an `egui` widget or a
`js:*` foreign value exists on one side only; the `ext-temporal` scalars exist only where the
feature is enabled.

Nothing expresses this today. `TypeRegistry` (`value-type-system`, `liquers-core/src/type_system.rs`)
describes **one** build's types, and it is reachable only from that build's `Environment`. A planner
on the frontend has no way to ask what the backend can hold, so it cannot tell that a step producing
`pl:dataframe` must not be scheduled locally, nor that a value crossing the boundary needs to become
something the other side understands.

`CommandMetadataRegistry` already carries a realm in `CommandKey { realm, namespace, name }`
(`command_metadata.rs:561`) and is shared across the boundary, so commands are half-prepared for
this and types are not prepared at all.

## Impact

Strategic rather than immediate, and it compounds with the type work rather than replacing it: as
long as one build's registry is authoritative, a cross-realm query fails at the moment of transfer
with a type the receiver has never heard of — which is exactly the "degrade to bytes with a warning"
path `value-type-system` specifies. Degrading is a correct floor, but it is not the transparent
behaviour a user should get: the right answer is usually "convert the frame to CSV before it
crosses", decided at planning time, not "hand the browser bytes it cannot open".

## Expected behaviour

1. ~~**The type registry is realm-aware.**~~ **Delivered by `value-type-system`**: the registry is
   keyed by `TypeKey { realm, type_identifier }` mirroring `CommandKey`, with a default realm so
   single-realm code is unaffected, plus `with_realm` and `get_in_realm`. One field and a
   convenience layer, so there was no reason to defer it — and adding a key component later would
   have rewritten every stored entry. What remains below is behaviour, which a field cannot carry.
2. **The registry is shared across realms**, the way command metadata is — so a planner can ask
   what any participating realm supports before scheduling a step there.
3. **Each type declares what happens where it is unsupported.** At minimum: convert to a named
   supported type or data format, or fail with a typed error naming the type and the realm. The
   conversion case depends on `VALUE-CONVERSION-CAPABILITY`.
4. **The plan places the conversion**, so a value that must cross a boundary is converted on the
   side that can do it, and the transfer is transparent to the query author.

Wants a design. It spans the type system, the planner and both bindings, and it should follow
`CORE-MULTI-REALM-INTERPRETER` — realm-aware *dispatch* has to exist before realm-aware *typing*
has anything to attach to.

## Discovery

Raised by the user during `value-type-system` Phase 2, 2026-08-18, while reviewing the
`TypeRegistry` architecture. Phase 2 was adjusted to key the registry in a realm-ready shape and to
give `TypeInfo` a builder so the unsupported-type action can be added without breaking construction;
see `specs/design/value-type-system/phase2-architecture.md`, "Forward compatibility".

Related: `CORE-MULTI-REALM-INTERPRETER` (P3, XL) — the interpreter dispatches in a single realm
(`plan.rs:1081`); this issue is the type-system half of the same programme.
`VALUE-CONVERSION-CAPABILITY` supplies the conversion the unsupported-type action needs.
