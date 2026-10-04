# Phase 2: Solution and Architecture - `Context::set_title` and `Context::set_description`

## Chosen Solution (recipe wins)

### `AssetData` (`liquers-core/src/assets.rs`)

Two private fields, initialised `false` in `AssetData::new` and cleared in `reset`:

```rust
/// The key's resolved recipe declared a non-empty title, which is then final: a command's
/// `Context::set_title` does not replace it. See `design/context-title-description/`.
recipe_sets_title: bool,
/// The same for the description.
recipe_sets_description: bool,
```

In the recipe-adoption block of `AssetRef::evaluate_recipe_outcome` (≈3036-3050), where
`metadata.with_title(recipe.title.clone()).with_description(recipe.description.clone())` runs:

```rust
lock.recipe_sets_title = !recipe.title.is_empty();
lock.recipe_sets_description = !recipe.description.is_empty();
```

### `AssetRef` (`liquers-core/src/assets.rs`)

A crate-private method next to `set_description_fields`:

```rust
/// Sets `title` and/or `description` from the command producing this asset, except a field
/// whose value came from the key's resolved recipe, which is kept. `Ok(())` either way.
pub(crate) async fn set_description_fields_from_command(
    &self,
    title: Option<String>,
    description: Option<String>,
) -> Result<(), Error> {
    let mut lock = self.data.write().await;
    let title = title.filter(|_| !lock.recipe_sets_title);
    let description = description.filter(|_| !lock.recipe_sets_description);
    if title.is_none() && description.is_none() {
        return Ok(());
    }
    set_metadata_description(&mut lock.metadata, title, description)
}
```

The flag check and the write are under one write lock, so a concurrent adoption cannot slip in
between. The early return means the legacy-metadata refusal only fires when something would be
written.

### `Context` (`liquers-core/src/context.rs`), beside `set_filename`

```rust
/// Sets the title of the asset this command is producing.
///
/// A title declared by the key's recipe takes precedence and is kept; the call then does
/// nothing and still returns `Ok`. Persisted with a keyed asset's metadata; never changes its
/// `version`. `NotSupported` on legacy metadata.
pub async fn set_title(&self, title: &str) -> Result<(), Error> {
    self.assetref
        .set_description_fields_from_command(Some(title.to_string()), None)
        .await
}

/// Sets the description of the asset this command is producing. Same rules as [`Self::set_title`].
pub async fn set_description(&self, description: &str) -> Result<(), Error> {
    self.assetref
        .set_description_fields_from_command(None, Some(description.to_string()))
        .await
}
```

### How a set value reaches storage (no change needed)

`AssetRef::evaluate` (≈3117) merges type identifier and dependencies into the live
`lock.metadata` instead of replacing it; `serialize_to_binary` clones that record
(`data.metadata.clone()`, ≈3409); `save_to_store` writes it. `version` comes from
`Version::from_content` over the bytes.

## Rejected Alternatives

- **Command wins (plain last write).** Rejected by the maintainer's decision.
- **Detect a recipe title by comparing strings with `lock.recipe.title`.** Ad-hoc placeholders and
  coincidental equality make it unreliable (Phase 1).
- **Return an error when the recipe wins.** A generic command cannot know whether a recipe
  exists for the key it runs under, so it would have to ignore the error everywhere.
- **Re-apply the recipe's title after the command finishes.** Equivalent result, but a second
  write path into metadata, and a window in which observers see the command's title.
- **Deliver through the service channel like log entries.** Asynchronous; could land after
  serialization and miss the store write.

## Files and Symbols

| File | Symbol | Change |
|---|---|---|
| `liquers-core/src/assets.rs` | `AssetData` fields, `AssetData::new`, `AssetData::reset` | two flags |
| `liquers-core/src/assets.rs` | `AssetRef::evaluate_recipe_outcome` adoption block | set flags |
| `liquers-core/src/assets.rs` | new `AssetRef::set_description_fields_from_command` | as above |
| `liquers-core/src/context.rs` | `Context::set_title`, `Context::set_description` | new |
| `liquers-core/tests/context_title_description.rs` | new | Phase 3 |

Check every other `AssetData { … }` struct literal (if any exist besides `new`) when adding the
fields: the compiler will list them.

## Errors, Ownership, Sync/Async

Borrowed `&str` cloned into `String`. Errors: `NotSupported` from legacy metadata, only when
writing. The write lock is brief and not held across an await. wasm: no spawn, works unchanged.

## API and Compatibility

Additive. `register_command!` unchanged (commands already receive `context`);
`command_registry.yaml` unaffected; `liquers-py`/`liquers-web` untouched.

## Questions

None open. Implementation details resolved here: per-field flags, silent `Ok` when the recipe
wins, empty string from a command clears an unrecipe'd field (as `with_title("")` would).

## Risk Table

| Aspect | Assessment |
|---|---|
| Likely files | `assets.rs`, `context.rs`, one new test file |
| Affected workflows/crates | command authors; listings of command-produced assets without recipe titles |
| Existing-test impact | none expected; recipe-title adoption tests keep passing (flags are additive) |
| New validation | unrecipe'd asset; recipe title kept; recipe-empty field filled and persisted; version invariance; legacy refusal; reset clears flags |
| Compatibility/data | stored metadata may carry command descriptions; format unchanged |
| Concurrency | flag check and write under one lock |
| Security | none |
| Recovery | remove the two methods; flags are inert without them |
| Certainty | high |

## Review

Against Phase 1: criterion 1 → `Context` signatures; 2 → unflagged write; 3 → flag filter and
`Ok`; 4 → per-field flag plus the existing persistence path; 5 → `Version::from_content`; 6 →
`set_metadata_description`. Against code: the adoption block, `reset`, `set_description_fields`,
`set_metadata_description`, `serialize_to_binary` and `Recipe`'s ad-hoc constructors were read
at HEAD.
