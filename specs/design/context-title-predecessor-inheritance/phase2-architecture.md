# Phase 2: Solution and Architecture

## Flags (`liquers-core/src/assets.rs`)

`AssetData` gains `command_set_title: bool` and `command_set_description: bool` (runtime only, not
serialized). `set_description_fields_from_command` sets the flag of each field it applies, after the
recipe filter. Add:

```rust
impl<E: Environment> AssetRef<E> {
    /// The title and description this asset's commands set (not its recipe's), for a consumer
    /// across a predecessor boundary.
    pub(crate) async fn command_set_fields(&self) -> (Option<String>, Option<String>)
}
```

It reads the values from the asset's metadata, filtered by the flags.

## Boundary (`liquers-core/src/interpreter.rs`, `Step::Evaluate`)

```rust
let query = context.resolve_query_from_cwd(&query)?;
let asset = context.submit(&query).await?;
let state = context.wait_for_dependency(&asset).await?;
let (title, description) = asset.command_set_fields().await;
if title.is_some() || description.is_some() {
    context.inherit_description_fields(title, description).await?;
}
state.value()
```

`Context::inherit_description_fields` is a crate-private wrapper that calls
`self.assetref.set_description_fields_from_command(title, description)`. It applies the
recipe-wins filter and marks the final asset's flags, so a further predecessor level inherits
transitively. Later `Step::Action`s calling `set_title` overwrite, which gives the "later step
wins" order.

Check that `submit` + `wait_for_dependency` are exactly what `get_dependency_state` does
(`context.rs` ≈757–760: `let asset = self.submit(query).await?; self.wait_for_dependency(&asset).await`).

## Alternative (keep)

No code. Close the issue as decided.

## Known-issue preflight

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `assets.rs`, `context.rs`, `interpreter.rs`, `tests/context_title_description.rs`, DOC_04 |
| Behaviour | Final assets gain titles/descriptions from predecessor commands (the intended change) |
| Concurrency | Read of the predecessor's lock after it finished |
| Recovery | Revert. The flags are inert without the boundary call. |
| Certainty | High once decided |
