# Phase 2: Solution and Architecture

## Change

At both lazy-expiry sites of `ImmediateAssetManager` (`liquers-core/src/assets.rs`), replace

```rust
let _ = assetref.expire_without_cascade(ExpiryReason::Direct {
    cause: ExpiryCause::Deadline { expiration_time },
}).await;
```

with

```rust
let _ = assetref.expire_with_reason(ExpiryReason::Direct {
    cause: ExpiryCause::Deadline { expiration_time },
}).await;
```

`expire_with_reason` is `pub(crate)` on `AssetRef`. It marks the asset expired (persisting the
status for a stored keyed asset, recording the reason), then calls
`manager.cascade_expire_dependents(&dep_key, cause)`. Update both comments to drop "whether it
should is open".

## Lock analysis

The keyed site calls it before `self.key_mutation_lock.lock().await`. The cascade goes through
`expire_without_cascade` and store writes on dependents, and neither takes the key-mutation lock.
The queued monitor relies on the same property.

## Residual case (documented, not changed)

`DEPENDENCIES_STATUS.md`: "On the immediate manager, expiry is discovered when the expired asset is
looked up, and then cascades. A dependent read before its root is looked up is served from its last
value; `trigger_dependency_audit` or `AuditPolicy::OnLoad` catch it."

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES` | Closed predecessor | No |
| `DEPENDENCY-EDGE-RECORDED-AGAINST-SUPERSEDED-VERSION-IS-NOT-EXPIRED` | Another in-process staleness window | No |

## Relevant commands

Tests register `make_a` with `expires: "in 1 sec"`.

## Documentation architecture

`DEPENDENCIES_STATUS.md` paragraph, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` (2 sites); `tests/common/manager_scenarios.rs`; `DEPENDENCIES_STATUS.md` |
| Workflows | Browser freshness; any immediate-manager deployment |
| Existing tests | A test pinning "dependent stays Ready" on the immediate manager flips. Search `expire_without_cascade` and `Deadline` in core tests. |
| Compatibility | More recomputation after deadlines, which is the intended freshness |
| Performance | One graph walk per lazily expired keyed root |
| Recovery | Revert both sites |
| Certainty | High |
