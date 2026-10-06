# Phase 2: Solution and Architecture

In `AssetRef::enter_dependencies` (`liquers-core/src/assets.rs`), after the edge is recorded with
`add_dependency(&current_dep_key, &dep_key, version)`:

```rust
// Stale at birth: the dependency changed after this asset read it and before the edge existed,
// so the cascade of that change could not reach it. Take the stale-dependency route.
let current = manager.dependency_manager().get_version(&dep_key).await;
if let Some(current) = current {
    if !version.is_unknown() && !current.is_unknown() && current != version {
        self.note_expired_dependency(dependency).await?;
    }
}
```

`get_version` returns `Option<Version>` (it is used that way above at the same site). The check
runs only on the branch where an edge is actually recorded (`current_dep_key` is `Some`).

Lock discipline: no `data` lock is held at that point (the metadata write lock was released at the
end of the preceding block). `note_expired_dependency` takes the dependency's read lock and then
the parent's write lock, as documented.

## Rejected alternatives

- Make `add_dependency` verify and return the dependent as expired. This reverses a pinned
  contract, and expiring a still-running dependent is the hazard the issue warns about.
- Re-read the dependency just before recording. That narrows the window but does not close it.

## Known-issue preflight

None blocking.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs`; a new integration test |
| Workflows | Concurrent writes during evaluation (listings, keyed deps) |
| Existing tests | Tests that deliberately record mismatched versions through evaluation would now see `Expired`. Search for tests registering a version between dependency and dependent. |
| Concurrency | Read-only map lookup, plus an existing labelling route |
| Performance | One map lookup per recorded edge |
| Recovery | Revert the block |
| Certainty | High |
