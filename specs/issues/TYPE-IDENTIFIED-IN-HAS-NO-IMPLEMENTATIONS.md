---
id: TYPE-IDENTIFIED-IN-HAS-NO-IMPLEMENTATIONS
kind: issue
title: No type implements TypeIdentifiedIn, so the documented to_type_identifier example does not compile
status: closed
priority: P3
complexity: S
area: [core/value, lib/value, docs]
design: type-identified-in-docs
created: 2026-10-20
github:
---

## Problem

`liquers-core/src/type_system.rs` defines `TypeIdentifiedIn<V>` and `to_type_identifier::<V, T>()`,
with blanket impls for `Arc<T>` and `&T` only. No type implements it anywhere in the workspace
(`grep -rn TypeIdentifiedIn` finds only the definition). Yet `specs/guides/TYPE_SYSTEM_GUIDE.md`
§"Naming a type in Rust code" shows `to_type_identifier::<ExtValue, polars::frame::DataFrame>()   //
"polars.DataFrame"`, and `specs/reference/VALUE_TYPE_SYSTEM.md` §"Rust types and identifiers"
describes the bridge as in use. The example fails to compile.

## Problem Example

Following `TYPE_SYSTEM_GUIDE.md`, `to_type_identifier::<ExtValue, polars::frame::DataFrame>()`
fails with `E0277: the trait bound DataFrame: TypeIdentifiedIn<ExtValue> is not satisfied`.
Should be: either it returns `"polars.DataFrame"`, or the guide does not present it as available.
Verified at HEAD (2026-10-26): only the definition and the `Arc<T>` / `&T` blanket impls exist.

## Impact

A reader following the guide gets a trait-bound error. Command registration cannot yet derive an
argument's identifier from its Rust type, which `VALUE-CONVERSION-CAPABILITY` and
`COMMAND-INPUT-OUTPUT-TYPES-ARE-UNDECLARED` assume.

## Expected behaviour

Either implement `TypeIdentifiedIn<ExtValue>` for the `ExtValue` payload types (`image::DynamicImage`,
`polars::frame::DataFrame`, …) and the core payloads — which `value-type-definition-macro` generates
— or, until then, say in the guide and the reference that no impls exist yet.

## Discovery

Bulk design of `VALUE-TYPE-DEFINITION-MACRO` (2026-10-20).

## Resolution

Fixed 2026-10-27 by `design/type-identified-in-docs/`. Fixed by `design/type-identified-in-docs/` (2026-10-27): `TYPE_SYSTEM_GUIDE.md`, `VALUE_TYPE_SYSTEM.md` and the trait rustdoc say no value type implements `TypeIdentifiedIn` yet. The implementations belong to `design/value-type-definition-macro/`.
