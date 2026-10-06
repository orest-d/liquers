# Phase 2: Solution and Architecture

## Change

```rust
let prefix = prefix_key.unwrap_or_else(Key::new);
let result = store.listdir_keys_deep(&prefix).await;
```

Update the doc comment: "every key under `prefix` (default: the whole store), at any depth".

## Alternative (if "synonym" is chosen)

Keep the code. Doc comment: "the keys directly in `prefix`; the same as `listdir`". Leave the spec
unchanged except for removing the issue reference.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `CORE-STORE-ROUTER-KEYS-FAILS-ON-AN-EMPTY-MEMBER` | A router store's deep listing can fail on an empty member | No; this is the same failure as `keys()` already has |

## Relevant commands

None.

## Documentation architecture

Spec row, History, `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `store/handlers.rs`; `tests/store_api_routes.rs`; spec |
| Compatibility | Responses grow for nested prefixes |
| Performance | Proportional to subtree size |
| Recovery | Revert |
| Certainty | High |
