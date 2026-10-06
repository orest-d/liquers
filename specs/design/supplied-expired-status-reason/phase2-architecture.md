# Phase 2: Solution and Architecture

## Change

At each of the four write sites (after `final_status` is decided and the metadata clone exists,
before validation and the store write):

```rust
if final_status == Status::Expired && metadata.expiry_reason().is_none() {
    self.record_expiry(
        &mut metadata,
        &key.to_string(),
        &ExpiryReason::Direct { cause: ExpiryCause::Explicit },
    );
}
```

For `set_binary` the metadata is a `MetadataRecord` local. Wrap it as
`Metadata::MetadataRecord`, or call the record's setter directly, mirroring what the default
`record_expiry` does. To keep one code path, convert to `Metadata` at that site. Check the
`expiry_reason` accessor name on `MetadataRecord` vs `Metadata` in `metadata.rs` before writing
the step.

A helper avoids repeating this four times:

```rust
fn ensure_supplied_expiry_reason<M: AssetManager<E> + ?Sized, E: Environment>(
    manager: &M, key: &Key, metadata: &mut Metadata)
```

## Errors

None new. `record_expiry` is infallible (best effort).

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM` | Same four sites | No (order before) |
| `EXTERNAL-MANAGER-CANNOT-NOTIFY-REPLACED-ASSET` | `new_installed` with `Expired` leaves the reason to its caller | No |

## Relevant commands

None.

## Documentation architecture

One sentence in the reference that states the reason invariant, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` |
| Existing tests | Tests that write `Expired` and compare the whole metadata may see an added log entry and reason. Search for `Status::Expired` in `set_state`/`set_binary` tests. |
| Data | Stored records gain a reason field (an optional serde field already exists) |
| Recovery | Revert |
| Certainty | High once decided |
