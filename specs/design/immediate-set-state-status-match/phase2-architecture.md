# Phase 2: Solution and Architecture

## Function

In `liquers-core/src/assets.rs`, next to `live_status_defers_to_store`:

```rust
/// The status a supplied value is written with (`set_binary`, `set_state`, both managers).
/// `Some` when the supplied status decides it; `None` when it depends on whether the key has a
/// recipe (then: `Override` with a recipe, `Source` without — see [`written_status_with_recipe`]).
fn written_status(supplied: Status) -> Option<Status> {
    match supplied {
        Status::Expired => Some(Status::Expired),
        Status::Error => Some(Status::Error),
        Status::None
        | Status::Directory
        | Status::Recipe
        | Status::Submitted
        | Status::Dependencies
        | Status::Processing
        | Status::Partial
        | Status::Storing
        | Status::Ready
        | Status::Cancelled
        | Status::Source
        | Status::Override
        | Status::Volatile => None,
    }
}

fn written_status_with_recipe(has_recipe: bool) -> Status {
    if has_recipe { Status::Override } else { Status::Source }
}
```

Each of the four sites becomes:

```rust
let final_status = match written_status(input_status) {
    Some(status) => status,
    None => written_status_with_recipe(self.recipe_opt(key).await?.is_some()),
};
```

The `match` on `Option` is exhaustive and has no default arm, and the recipe lookup stays lazy.

## Alternatives

A trait method on `AssetManager` was rejected. It is not part of the manager contract. External
managers may copy the two functions (documented in `external-manager-replacement-surface`).

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON` | Changes the `Expired` row later | No |

## Relevant commands

None.

## Documentation architecture

None.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` only |
| Existing tests | Unchanged expectations |
| New validation | Table test |
| Compatibility / data | None, same statuses written |
| Concurrency / performance | None, laziness kept |
| Recovery | Revert |
| Certainty | High |
