---
id: FOREIGN-VALUE-TYPES-NOT-REGISTERED
kind: issue
title: A foreign language value cannot be registered in the type registry
status: draft
priority: P1
complexity: M
area: [core/value, lib/value, web, py]
design: value-type-system
created: 2026-08-18
github:
---
## Problem

`TypeRegistry` is built once at `Environment` construction from `ValueInterface::type_descriptions`
(`liquers-core/src/context.rs`), which is a **static** associated function. A foreign value —
`ExtValue::Foreign`, holding a JavaScript, Python or Starlark handle — supplies its identifier at
*runtime* through `ForeignValue::identifier` (`liquers-lib/src/value/foreign.rs:48`), so it cannot
appear in that static list.

Since `value-type-system` step 6, the write path refuses an identifier the registry does not
contain. A foreign value therefore cannot be stored: `AssetManager::set_binary` and `set_state`
reject it with "Type identifier 'js.Value' is not registered in this build".

## Impact

Any language integration that puts a value into an asset is affected — `liquers-web`'s `JsValue`
handles most directly. The failure is a clean typed error rather than corruption, but it is a
regression against the previous behaviour, where an unserializable or unknown value degraded to
metadata-only persistence.

**Not verified against a build.** `liquers-web` is `wasm32`-only and the `wasm32-unknown-unknown`
target is not installed in the environment where this was found, so the rejection is derived from
reading the write path rather than observed. Confirming it is the first step.

## Expected behaviour

A type known only at runtime can be registered. Two shapes are plausible and the choice is a design
decision:

1. **A mutable registration point** — `Environment` exposes a way to add a `TypeInfo` after
   construction, so an integration registers its language's types at startup. This conflicts with
   the registry being built once and read-only thereafter, which is what lets it be shared without
   a lock.
2. **A registered *family*** — one `TypeInfo` per language (`js`, `py`) that accepts any local name
   under that provider, so `js.Value` and `js.Uint8Array` are covered by a single declaration. This
   preserves the read-only registry and fits the `provider.LocalName` naming rule, but weakens the
   guarantee from "this exact type is known" to "this provider is known".

Option 2 looks closer to the design's grain and is cheaper; option 1 is more precise. Either way
`ForeignValue` should gain a `type_info` with a default derived from its existing `identifier` and
`default_*` methods, so no integration has to write one by hand.

## Discovery

Found on 2026-08-18 during `value-type-system` step 8, while giving `ExtValue` its type
descriptions. The same class of problem surfaced for `ExtValue::UIElement` and was solved there by
exempting types that declare no data formats from the *format* check — but that exemption does not
help here, because the failure is the *identifier* check, which is deliberately stricter.
