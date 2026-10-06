# Phase 2: Solution and Architecture

## Change

In `ImmediateAssetManager` (`liquers-core/src/assets.rs`), at both lazy-expiry sites (the query
map loop in `get_asset` and the keyed lookup loop), replace

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

`expire_with_reason` is `pub(crate)` on `AssetRef`, and `ImmediateAssetManager` is in the same
crate. It marks the asset expired (persisting the status for a stored keyed asset, recording the
reason) and calls `manager.cascade_expire_dependents(&dep_key, cause)`. Update the comment at each
site to remove the "whether it should is open" sentence.

## Lock analysis

At the keyed site, the call happens before `self.key_mutation_lock.lock().await`. The cascade
expires dependents through `expire_without_cascade` and their store writes, and neither takes the
manager's key-mutation lock (the queued monitor relies on the same property). No new lock order is
introduced.

## Residual case

Not addressed in code (Phase 1, question 2a). `DEPENDENCIES_STATUS.md` states: "On the immediate
manager, a dependent read before its expired root is touched is served from its last value; run
`trigger_dependency_audit` or use `AuditPolicy::OnLoad` when this matters."

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `IMMEDIATE-MANAGER-LAZY-DEADLINE-EXPIRY-NEVER-FIRES` | Closed predecessor; made the lazy path reachable | No |

## Relevant commands

None new. Tests register a command with `expires: "in 1 sec"` through `register_command!`'s
`expires:` statement.

## Documentation architecture

`DEPENDENCIES_STATUS.md`: change the immediate-manager expiry statement and add the residual-case
sentence, with History and `reviewed:`.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs` (2 sites); `liquers-core/tests/common/manager_scenarios.rs`; `DEPENDENCIES_STATUS.md` |
| Workflows | Browser (`liquers-web`) freshness; any immediate-manager deployment |
| Existing tests | A test asserting the dependent stays `Ready` on the immediate manager (from the provenance design, Example 6) changes expectation. Search `expire_without_cascade` and `Deadline` in core tests. |
| New validation | Shared scenario (Phase 3) |
| Compatibility | More recomputation after deadlines, which is the intended freshness |
| Concurrency | See the lock analysis |
| Performance | One graph walk per lazily expired keyed root |
| Recovery | Revert the two call sites |
| Certainty | High for question 1 |
