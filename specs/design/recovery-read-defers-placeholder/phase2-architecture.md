# Phase 2: Solution and Architecture

## Chosen solution

In both default methods of `trait AssetManager` (`liquers-core/src/assets.rs`, `get_any_status`
and `get_binary_any_status`), replace

```rust
if let Some(asset_ref) = self.lookup_key_asset(key) {
    return Ok(asset_ref.get_any_status().await);
}
```

with

```rust
if let Some(asset_ref) = self.lookup_key_asset(key) {
    if !live_status_defers_to_store(asset_ref.status().await) {
        return Ok(asset_ref.get_any_status().await);
    }
    // A placeholder that has produced nothing yet: the store decides (as in `remove`).
}
```

The binary method gets the same change, with `get_binary_any_status().await` returned as is. The
store path below the `if` is unchanged.

## Signatures

None change. `live_status_defers_to_store(status: Status) -> bool` is a private free function in
the same module and already lists every variant explicitly.

## Rejected alternatives

- **Have `get` load the stored value before mapping the placeholder.** That reorders the keyed
  lookup, which several invariants depend on (the key-mutation lock, the dependency hand-off). It
  is far more invasive than a read-side fix.
- **Wait on the placeholder.** A recovery read must not trigger or await evaluation (its
  documented contract).

## Ownership, async and errors

There is no new lock. `status()` takes the asset's read lock briefly, and that lock is released
before the store is read, which preserves the existing "no asset lock held during store I/O"
property of the store path. Errors are unchanged: store errors propagate as before.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `METADATA-ONLY-ENTRY-RELOADS-AS-CORRUPTED` | The store path deserializes the stored bytes. A metadata-only entry already reaches the store path today when nothing is mapped. | No |
| `MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES` | Same as above. | No |

## Relevant commands

None. This is not a command-level change.

## Documentation architecture

`specs/reference/ASSETS.md`, recovery reads: add "A live asset that has not produced anything yet
(`None`, `Recipe`) defers to the store, as `remove` does." This needs a History row and a
`reviewed:` bump.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` (two methods, tests module); `specs/reference/ASSETS.md` |
| Workflows/crates | Recovery reads in core; the axum Assets API inherits the fix |
| Existing tests | Recovery-read tests keep passing. Any test asserting `None` for a mapped placeholder would be asserting the bug (none found by searching for `get_any_status` in the tests). |
| New validation | Two unit tests (Phase 3) |
| Compatibility | Callers get a value where they previously got `None`. No caller relies on `None` (it was a race outcome). |
| Concurrency | Non-atomic status-then-store read, discussed in Phase 1; benign |
| Performance | One extra status read on the live path |
| Security | None |
| Recovery | Revert the two hunks |
| Certainty | High |
