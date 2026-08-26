---
id: ERROR-STATE-FROM-ERROR-NOT-STORABLE
kind: issue
title: A state built by State::from_error cannot be stored — its type_name is empty
status: draft
priority: P1
complexity: S
area: [core/value, core/assets]
design:
created: 2026-08-26
github:
---
## Problem

`State::from_error` (`liquers-core/src/state.rs:103`) produces a state the write path refuses.

```rust
pub fn from_error(error: Error) -> Self {
    let mut metadata = Metadata::new();
    metadata.with_error(error);
    let data = Arc::new(V::none());
    Self::sync_metadata_with_value(&mut metadata, &data);   // returns early for an error state
    …
}
```

`Metadata::with_error` (`metadata.rs:1898`) sets `type_identifier` to `"error"` but leaves
`type_name` at its `Default` — the empty string. `sync_metadata_with_value`
(`state.rs:37`) is what would normally fill both in from the value, and it deliberately returns
early when `is_error` is set, so nothing ever supplies a `type_name`.

Since `value-type-system` step 6, `validate_required_fields` (`assets.rs:533`) refuses metadata
whose `type_name` is empty. So:

```
State::from_error(...) -> set_state(...)
  => [General] Metadata type_name must not be empty
```

**Verified 2026-08-26** by storing such a state through a `DefaultEnvironment` with a memory store:
`type_identifier = "error"`, `type_name = ""`, and `set_state` refused.

## Impact

**Any errored asset persisted through `State::from_error` fails to store.** The error state is
exactly the thing the `error` pseudo-type was registered to make storable, so this defeats that
work at the last step.

The message makes it worse: it names `type_name`, not the error being recorded, so the symptom
reads as a metadata bug in whatever produced the state rather than as a missing constant in the
error constructor. Anyone hitting it will look in the wrong place first.

The asset manager's own failure path is unaffected — `lock.metadata.with_error(e)`
(`assets.rs:1309`, `:2321`, `:3166`) mutates a record that already carries the original type's
`type_name`, so it stays non-empty. It is the *fresh* construction that is broken:
`State::from_error`, `Metadata::from_error` and `MetadataRecord::from_error`
(`metadata.rs:1053`, `:1499`) — and `MetadataRecord::from_error` is worse still, leaving
`type_identifier` empty too, because the record-level `with_error` does not set it either.

## Expected behaviour

An error state carries both halves of its type. `Metadata::with_error` already knows the
identifier is `"error"`; it should set `type_name` in the same place, so the two cannot diverge.
`TypeInfo::error()` (`type_system.rs:189`) already names the type `"error"`, which is the value to
use. `MetadataRecord::with_error` should set both as well, so the record-level and enum-level
constructors agree.

A test storing a `State::from_error` through an asset manager would have caught this and does not
exist.

## Discovery

Found on 2026-08-26 while implementing `foreign-value-type-registration` step 6, writing a test
that an environment built from an *empty* registry cannot store an errored asset. The test failed
for the wrong reason — the write was refused before the registry was ever consulted — which
exposed this. Filed rather than fixed: the fix is one or two lines but it is a P1 on a different
path, and it deserves its own review rather than riding along inside a design about foreign value
types.
