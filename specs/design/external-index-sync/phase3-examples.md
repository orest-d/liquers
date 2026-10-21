# Phase 3: Examples & Use-cases — Asset expiration events observable by scope

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | An index feeder refreshing a folder | scope subscription and lag handling | AC-1, AC-3 |
| 2 | Test | `asset_events::events01_cascade_to_live_dependent` | scoped, no handle held | AC-1, AC-6, AC-7 |
| 3 | Test | `asset_events::events02_stored_only_cascade_is_announced` | `expire_stored_copy` | AC-2, AC-7 |
| 4 | Test | `asset_events::events03_explicit_expire_of_stored_key` | `AssetManager::expire` stored branch | AC-2, AC-6 |
| 5 | Test | `asset_events::events04_lag_is_reported` | buffer overflow | AC-3 |
| 6 | Test | `asset_events::events05_sequence_root_first` | ordering, no coalescing | AC-4 |
| 7 | Test | `asset_events::events06_scope_filter` | `Under` vs `All`, query subjects | AC-5 |
| 8 | Test | `asset_events::events07_deadline_reason` | `Direct { Deadline }` | AC-6 |
| 9 | Test | `asset_events::events08_no_subscriber_unchanged` | no effect without subscribers | AC-8 |
| 10 | Test | `external_asset_manager::external_manager_announces_expiry` | external manager | AC-7 |

## Example 1: an index feeder refreshing a folder

```rust
let mut events = envref.get_asset_manager()
    .subscribe_asset_events(AssetEventScope::Under(parse_key("docs")?));
loop {
    match events.recv().await {
        AssetEventReceive::Event(AssetEvent::Expired { subject: AssetEventSubject::Key(k), .. }) => {
            sink.refresh(&k).await?;               // re-read through the manager, re-index
        }
        AssetEventReceive::Event(AssetEvent::Expired { subject: AssetEventSubject::Query(_), .. }) => {}
        AssetEventReceive::Lagged(_) => sink.reconcile(&parse_key("docs")?).await?,   // full diff
        AssetEventReceive::Closed => break,
    }
}
```

The `Lagged` arm is the design's point: a missed event becomes a reconciliation, never a silent
gap.

## Edge and Error Cases

- **Anonymous asset (ad-hoc `apply`)** → not announced (no subject) → events06 applies one and
  asserts nothing for it under `All`.
- **Store write of a stored-only expiry fails** → no event (the expiry did not happen) → covered by
  the order in Phase 2; not separately tested, because the memory store does not fail.
- **Manager dropped** → `Closed` → events08 drops the environment and receives `Closed`.
- **wasm32** → same code; build checked by `scripts/check-build-matrix.sh`.

## Test Plan

New file `liquers-core/tests/asset_events.rs`. Tests 2-9 run on `SimpleEnvironment` and
`ImmediateEnvironment` (AC-7), with an `AsyncMemoryStore` holding `recipes.yaml`: `data/b.txt` is
`-R/data/a.txt/-/upper`, `other/c.txt` is `-R/other/x.txt/-/upper`. Commands: `upper`, registered by
`register_provenance_commands` in `tests/common/manager_scenarios.rs`. Every `recv` is under `tokio::time::timeout(2s)`.

- `events01_cascade_to_live_dependent` (AC-1, AC-6, AC-7): get `data/b.txt`, drop the handle (it
  stays registered), subscribe `Under(data)`, `set_state` on `data/a.txt`. Receive `Expired` for
  `data/b.txt`, `live: true`, `Cascaded { cause: Updated { .. }, root, via }` equal to the stored
  metadata's `expiry_reason()`, `version` equal to the version before.
- `events02_stored_only_cascade_is_announced` (AC-2): as events01, after evicting `data/b.txt`
  (`remove_key_asset_if`, the live copy only). `live: false`.
- `events03_explicit_expire_of_stored_key` (AC-2, AC-6): stored-only `data/b.txt`;
  `m.expire(&key)`; `Direct { Explicit }`.
- `events04_lag_is_reported` (AC-3): 1100 explicit expiries of distinct stored keys without
  receiving; the first `recv` is `Lagged(n)` with `n >= 76`, then events follow in order.
- `events05_sequence_root_first` (AC-4): explicit expire of live `data/a.txt` (keyed, with
  dependent `data/b.txt`); the events arrive as `data/a.txt` then `data/b.txt`. Then recompute
  `data/a.txt`, expire again: two more events, none coalesced.
- `events06_scope_filter` (AC-5): changes under `data/` and `other/`, and the expiry of a
  registered query asset `-R/data/a.txt/-/upper` (a dependent recorded by query); `Under(data)` sees
  only `data/` keys; `All` sees all three kinds.
- `events07_deadline_reason` (AC-6): a recipe with `expires: "in 1 second"` on the queued manager
  (monitor) and the inline manager (lazy check on `get`, under `tokio::time` real time); the
  reason is `Direct { Deadline { .. } }`.
- `events08_no_subscriber_unchanged` (AC-8): the cascade scenario with no subscriber passes as
  today, then a subscriber created afterwards receives `Closed` once the environment is dropped
  (requires `environment-manager-reference-cycle`; until then the test asserts only the first half).
- `external_manager_announces_expiry` (AC-7), in `liquers-core/tests/external_asset_manager.rs`:
  events01 on `MinimalInlineAssetManager`.

Queries used above validate with `liquers-validate --command upper` (`upper` is test-only):
`-R/data/a.txt/-/upper` → `GetAsset[data, a.txt], Action{upper}`.

Command: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test asset_events --test
external_asset_manager`.

## Learning Log

- Every expiry route already funnels through three `record_expiry` callers. That funnel, made for
  expiry provenance, is what makes a complete event stream cheap.
