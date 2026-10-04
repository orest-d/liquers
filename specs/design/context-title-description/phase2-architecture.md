# Phase 2: Solution and Architecture - `Context::set_title` and `Context::set_description`

## Chosen Solution

In `liquers-core/src/context.rs`, beside `set_filename`:

```rust
/// Sets the title of the asset this command is producing.
///
/// Overrides a title its recipe declared: the recipe's metadata is applied before the command
/// runs, so the command's call is the later write. Persisted with a keyed asset's metadata; does
/// not change its `version`. `NotSupported` on legacy metadata.
pub async fn set_title(&self, title: &str) -> Result<(), Error> {
    self.assetref
        .set_description_fields(Some(title.to_string()), None)
        .await
}

/// Sets the description of the asset this command is producing. Same rules as [`Self::set_title`].
pub async fn set_description(&self, description: &str) -> Result<(), Error> {
    self.assetref
        .set_description_fields(None, Some(description.to_string()))
        .await
}
```

`AssetRef::set_description_fields` (`assets.rs` ≈3530) takes the asset's data write lock and
calls the module-private `set_metadata_description`, which writes `MetadataRecord::title` /
`description` and refuses `LegacyMetadata`. Its doc comment ("Used by
`AssetManager::set_description`, which checks the status first") gains "and by
`Context::set_title` / `set_description`, which run inside the asset's own evaluation".

Why it reaches storage with no further change: `AssetRef::evaluate` (≈3117) merges into
`lock.metadata`; `serialize_to_binary` builds the stored metadata from the live record; then
`save_to_store` writes it. Version is computed from bytes (`Version::from_content`).

## Rejected Alternatives

- **Write `lock.metadata` directly in `Context` like `set_filename`.** Duplicates the
  legacy-metadata rule; reuse is better.
- **Send through the service channel (`AssetServiceMessage`) like log entries.** Asynchronous
  delivery could land after the value is installed and serialized, so the title might miss the
  store write. Rejected: direct, awaited write.
- **Recipe-wins precedence.** Needs remembering the recipe's values and re-applying after the
  command or refusing the call; recorded as the alternative in Phase 1, not implemented.

## Files and Symbols

| File | Symbol | Change |
|---|---|---|
| `liquers-core/src/context.rs` | `impl<E: Environment> Context<E>` | add `set_title`, `set_description` |
| `liquers-core/src/assets.rs` | `AssetRef::set_description_fields` | doc comment only |
| `liquers-core/tests/` | new `context_title_description.rs` | Phase 3 |

## Errors, Ownership, Sync/Async

Borrowed `&str`, cloned into owned `String`. Errors: `NotSupported` from legacy metadata. The
write lock is short and not held across awaits. A command must call it from an async body or
via the context in an async command (same as `set_filename`). wasm: no spawn, works unchanged.

## API and Compatibility

Additive. `register_command!` needs no change (commands already receive `context`).
`command_registry.yaml` unaffected. `liquers-py`/`liquers-web` untouched.

## Questions

- **Open design question - precedence:** see Phase 1; implementation follows the recommendation.
- **Implementation detail - empty string:** sets the field to empty (clears it), as
  `MetadataRecord::with_title("")` would; documented.

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `context.rs`, a doc comment in `assets.rs`, one new test file |
| Affected workflows/crates | command authors; listings after evaluation |
| Existing-test impact | none — new API |
| New validation | unkeyed metadata, keyed stored metadata + reload, recipe override, version invariance, legacy refusal |
| Compatibility/data | stored metadata may carry command titles; format unchanged |
| Concurrency | data lock contention with the service loop's progress writes — same lock, brief |
| Security | none |
| Recovery | delete the two methods |
| Certainty | high for mechanics; precedence pending decision |

## Review

Against Phase 1 criteria: 1 → signatures; 2-3 → merge + persist path inspected; 4 → ordering in
`evaluate_recipe_outcome`; 5 → `Version::from_content`; 6 → `set_metadata_description`.
