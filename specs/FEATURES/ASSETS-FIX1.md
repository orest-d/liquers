# ASSETS-FIX1

Status: Draft

## Summary
`ASSETS-FIX1` consolidates all `TODO`, `FIXME`, and `todo!()` markers in `liquers-core/src/assets.rs` into a concrete implementation backlog.  
Focus: remove known runtime gaps (dependency handling, delegation deadlock risk, metadata consistency), reduce duplication, and finalize incomplete API paths.


## Inventory (assets.rs)

| # | Fix? | Location | Marker | Proposed solution |
|---|---|---|---|---|
| 16 | Phase4 | `assets.rs:1233` | log string contains `FIXME` | Replace with structured debug log without FIXME marker. |
| 17 | Phase4 | `assets.rs:1237` | `FIXME` delegation can deadlock if not queued | Replace blocking delegation (`asset.get().await`) with dependency scheduling + non-blocking parent wait state; ensure delegated asset submitted before parent waits. |
