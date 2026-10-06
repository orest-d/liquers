# Phase 2: Solution and Architecture

## Recommended solution

`liquers-core/src/assets.rs`:

1. Change `pub(crate) async fn notify_removed(&self)` to `pub`. Expand the doc comment into a
   contract: "Call once, after `cancel()`, before unmapping the asset; the last message a
   subscriber sees."
2. Add:

   ```rust
   impl<E: Environment> AssetRef<E> {
       /// A finished keyed asset holding `state`, for an [`AssetManager`] installing a value
       /// (`set_state`). `status` must be one a write can produce: `Source`, `Override`,
       /// `Expired` or `Error`; anything else is `ErrorType::General` naming the status.
       /// No service loop is started and nothing is written to the store.
       pub fn new_installed(
           id: u64,
           key: Key,
           state: State<E::Value>,
           status: Status,
           envref: EnvRef<E>,
       ) -> Result<Self, Error>
   }
   ```

   The body uses `AssetData::new_ext(id, (&key).into(), State::new(), Some(key), envref)`, then
   sets `data = Some(state.data)`, `metadata = state.metadata` with the status applied,
   `status = status`, `binary = None`, and calls `to_ref()`. The status check is an explicit
   `match` over every `Status` variant.

## Alternative (if the surface stays closed)

No code. Rewrite the guide row as a documented limit with its rationale, and close the issue as
`closed_not_planned`.

## Ownership and async

`new_installed` is synchronous: no lock is contended on a fresh `AssetData`. `state.data` is
already an `Arc`. Cloning the `Arc`s is cheap, and the value is never deep-copied (the built-in
`set_state` deep-copies today with `.as_ref().clone()`, which this does not need).

## Errors

Use `Error::general_error(format!("new_installed: status {status:?} is not a written status"))`.
No new `ErrorType`.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `IMMEDIATE-SET-STATE-STATUS-MATCH-HAS-DEFAULT-ARM` | Shares the written-status rule | No; order it first if both are scheduled |
| `SUPPLIED-EXPIRED-STATUS-STORED-WITHOUT-REASON` | `Expired` installed this way also lacks a reason | No; `new_installed` documents that the caller records the reason via `record_expiry` |

## Relevant commands

None.

## Documentation architecture

`ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`: add rows for `AssetRef::notify_removed()` and
`AssetRef::new_installed(..)` to the primitives table. Remove `notify_removed` from "Not exposed,
deliberately", delete the §11 limitation row, and add a History row and a `reviewed:` bump.

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `liquers-core/src/assets.rs`; `liquers-core/tests/common/minimal_manager.rs`; `manager_scenarios.rs`; the guide |
| Workflows | External managers only |
| Existing tests | None change |
| New validation | Minimal-manager scenarios (Phase 3) |
| Compatibility | Additive public API (semver-minor) |
| Concurrency | `notify_removed` takes a read lock and sends on a `watch` channel, which is safe from any task |
| Security | None |
| Recovery | Revert. Nothing else depends on it. |
| Certainty | High, once the decision is made |
