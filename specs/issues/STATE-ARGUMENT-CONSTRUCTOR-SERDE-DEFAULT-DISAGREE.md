---
id: STATE-ARGUMENT-CONSTRUCTOR-SERDE-DEFAULT-DISAGREE
kind: issue
title: Constructing and deserializing the same CommandMetadata give different state arguments
status: draft
priority: P2
complexity: S
area: [core/commands]
design: state-argument-serde-default
created: 2026-08-29
github:
---
## Problem

`CommandMetadata::state_argument` has two disagreeing defaults.

The constructors set it to `Some(ArgumentInfo::any_argument("state"))` —
`CommandMetadata::new` (`liquers-core/src/command_metadata.rs:896`) and
`CommandMetadata::from_key` (`:928`). The serde default is `Option::default()`, i.e. `None`, because
the field carries a plain `#[serde(default)]` (`:823-825`).

So a metadata document that omits `state_argument` deserializes into a command that takes **no**
state — a source command — where the equivalent constructor call produces one that takes a state.
Measured at `HEAD`:

```
CommandMetadata::new("greet").state_argument
  == Some(ArgumentInfo { name: "state", .. })

serde_json::from_str::<CommandMetadata>(
    r#"{"name":"greet","label":"Greet","cache":true,"volatile":false,"definition":"Registered"}"#
).unwrap().state_argument
  == None
```

This is not caught today because `specs/command_registry.yaml` writes `state_argument` explicitly
for every command that has one, so the round-trip never exercises the omitted case.

## Why it matters

`state_argument` is not decorative: `plan.rs` uses the presence of a state argument to decide how an
action consumes its predecessor. A command silently losing its state argument on deserialization
plans differently from the same command constructed in Rust. Any future path that reads command
metadata from a document — which is exactly what `COMMAND-DECLARATION-FORMAT` is building — inherits
the discrepancy.

It also makes the "what may a declaration omit?" question unanswerable: there is no single default
to point at.

## Fix direction

Decide which default is correct and make both paths agree. Two candidates:

1. **`None` is correct** (a command declares its state argument explicitly; omission means a source
   command). Then the constructors are wrong and should not fabricate one — but that changes every
   `CommandMetadata::new`/`from_key` caller, including `register_command!`, so it is the larger
   change.
2. **`Some(any_argument("state"))` is correct** (the common case is a transforming command). Then
   the field needs `#[serde(default = "...")]` returning it — but that makes "no state argument"
   inexpressible by omission, requiring an explicit `state_argument: null`.

Option 2 is the smaller change and matches the constructors; option 1 matches the exported registry's
explicitness. Either way the choice should be recorded, not left implicit.

Note the related `TODO: state argument should be optional` already in the source at `:821`.

## Related

- `COMMAND-DECLARATION-FORMAT` — found this while measuring what a declaration may omit. Its
  Phase 2 sidesteps the issue by making the state argument the binding's explicit decision, so this
  is **not** a blocker for that work, but the underlying inconsistency remains for anyone
  deserializing `CommandMetadata` directly.
- `COMMAND-METADATA-ENHANCEMENTS` — would touch the same field if IO typing lands.

## Verification

A test asserting that `CommandMetadata::new("x")` and the deserialization of its own serialized form
with `state_argument` omitted agree on `state_argument`.
