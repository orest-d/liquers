# Phase 2: Solution and Architecture

## Change

```rust
async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
    let key = key.as_absolute()?;
    if let Some(store) = self.find_store(key) {
        return store.get_metadata(key).await;
    }
    // Above the members: a directory that exists because a member is mounted below it.
    if self.is_dir(key).await? {
        let mut metadata = self.default_metadata(key, true);
        metadata.children = self.listdir_asset_info(key).await?;
        return Ok(Metadata::MetadataRecord(metadata));
    }
    Err(Error::key_not_found(key))
}
```

`listdir_asset_info` (trait default) calls `listdir_keys` and then `get_asset_info` on each child.
Children above deeper members recurse into this same branch, and children owned by a member
dispatch to it. Recursion depth is bounded by the key length. Check that the router's
`default_metadata` produces `is_dir = true` (the trait default sets it from the argument).

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | `listdir` of a member prefix that does not exist yet may fail inside `listdir_asset_info` | No. The test fixtures create the member prefixes. |

## Relevant commands

None.

## Documentation architecture

Guide §9 router row note.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/store.rs` (router + tests); guide §9 |
| Existing tests | Router conformance suite unchanged |
| Compatibility | An error becomes a listing. The axum store browser benefits. |
| Performance | Root metadata lists every member's top level |
| Recovery | Revert |
| Certainty | High |
