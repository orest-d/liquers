# Phase 3: Examples and Tests

| # | Kind | Checks |
|---|---|---|
| E1 | example | Minimal manager `set_state` with a UI-element-like non-serializable value, then `get` returns it (no evaluation) |
| E2 | example | A subscriber of the replaced asset receives `Removed` last |
| T1 | unit (`assets.rs`) | `new_installed` with each written status succeeds; each other status errors |
| T2 | integration | `minimal_manager` adopts `new_installed` in `set_state`; shared scenario `set_state_of_unserializable_value_is_served` passes for all managers |
| T3 | integration | Shared scenario `replaced_asset_receives_removed_last` passes for all managers |

## E1 sketch (minimal manager)

```rust
async fn set_state(&self, key: &Key, state: State<E::Value>) -> Result<(), Error> {
    let status = written_status(state.metadata.status(), self.has_recipe(key).await?);
    if let Some(old) = self.map.remove(key) { old.cancel().await?; old.notify_removed().await; }
    let asset = AssetRef::new_installed(self.next_id(), key.clone(), state.clone(), status, self.envref())?;
    if let Ok(bytes) = state.as_bytes() { store.set(key, &bytes, &metadata).await?; }
    else { store.set_metadata(key, &metadata).await?; }
    self.map.insert(key.clone(), asset);
    Ok(())
}
```

`written_status` is the shared rule from `immediate-set-state-status-match`. Before that design
lands, the minimal manager keeps its own explicit match.

## Non-serializable fixture

Use the core `Value` type's variant that has no byte form. If core has none, use a test
`ValueInterface` whose `as_bytes` always errors (`liquers-core/tests/common` already defines test
values; reuse one if present).
