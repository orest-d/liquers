# Phase 1: High-Level Design

## Design Readiness

- **Readiness:** needs-decision
- **Leading issue:** **Open design question — widen the external manager surface or stop it
  here.** `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` currently lists `notify_removed` as
  "not exposed, deliberately". Exposing it reverses a recorded decision.
- **Explanation:** A working design exists for each answer. Phases 3–4 specify the recommended
  one: expose the two primitives, guarded so they cannot bypass the status authority. The
  alternative ("document the limit") is a guide-only change, described in Phase 2 so it can be
  chosen without redesign.
- **Open questions:**
  1. **Proposed resolution — expose both:** make `AssetRef::notify_removed` public, and add a
     public `AssetRef::new_installed(...)` constructor that builds a finished asset from a `State`.
     It accepts only the statuses a built-in `set_state` can produce (`Source`, `Override`,
     `Expired`, `Error`). Recommended: neither primitive writes a raw status into a live asset,
     which was the reason given for keeping the list closed.

## Plain-language explanation (for the decision)

An `AssetManager` is the component that keeps the live assets (in-memory values, running
evaluations) for each key. Liquers ships two: the queued one (native) and the immediate one
(browser). Part F of `dependency-audit-and-expiry-provenance` made it possible to write a **third
manager outside `liquers-core`**, for example a manager that shares assets across a cluster or
proxies a remote server. It did that by making the lifecycle building blocks public. None ships
today. The only one is the test fixture `liquers-core/tests/common/minimal_manager.rs`.

Two things such a manager still cannot do, because the building blocks are private:

1. **Say goodbye to a replaced asset.** When someone writes a new value for a key
   (`set_state`/`set_binary`), the old live asset is replaced. A UI or websocket client watching
   the old asset should then receive a final "Removed" message, so it re-subscribes to the new
   one. The built-in managers send it. An external one can only cancel the old asset, so the
   watcher sees "Cancelled" and may show an error instead of switching to the new value.
2. **Hold a value that cannot be written to bytes.** The built-in managers can keep, say, a UI
   widget in memory under a key. An external manager can only store things that serialize, so it
   refuses such values.

The decision: open these two doors (two small public functions, guarded so they cannot set an
arbitrary status), or declare that external managers have this narrower contract and document it.
There is no user-visible effect either way until someone writes an external manager. Opening them
is cheap now and avoids a breaking change later. Keeping them closed keeps the public surface
smaller.

## Problem

Part F of `dependency-audit-and-expiry-provenance` made the lifecycle primitives an external
`AssetManager` needs public. Two things the built-in managers do in `set_binary` / `set_state` stay
crate-private:

- `AssetRef::notify_removed` sends a replaced asset its terminal `Removed` notification. An
  external manager can only `cancel()` the old asset, so a subscriber sees `Cancelled` and never
  `Removed`.
- Installing a new asset that holds a ready `State` needs `AssetData`'s private `data`, `metadata`
  and `status` fields. An external manager can only store bytes and let the next `get` fast-track
  them. A non-serializable value is therefore refused.

`liquers-core/tests/common/minimal_manager.rs` documents both limits.

## Expected behaviour and acceptance

1. A minimal external manager's `set_state` with a non-serializable value succeeds, and a
   subsequent `get` returns that value without evaluation.
2. A subscriber to a replaced asset sees `AssetNotificationMessage::Removed` as its last message,
   on the external manager as on the built-ins.
3. `new_installed` refuses an in-flight status (`Processing`, …) with a typed error.
4. The shared manager scenarios (`tests/common/manager_scenarios.rs`) run the replacement scenario
   against the minimal manager.

## Scope and non-goals

- No change to the built-in managers' behaviour. They may adopt `new_installed` internally to
  remove their duplicated construction. That is optional and is not required for this design.
- Not in scope: exposing claims, the job queue, `set_status`, `set_value` or `fail_asset`.

## Design Dependencies

- `dependency-audit-and-expiry-provenance` — **overlaps** (complete). It defined Part F's public
  surface.
- `immediate-set-state-status-match` — **overlaps**. The status rule a built-in `set_state` applies
  (`final_status`) becomes a shared function there. `new_installed` should validate against the
  same rule, so implement that design first or together.

## Documentation assessment

- Reference: none new. `ASSETS.md` does not list the manager-facing surface.
- Guide: extend `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §"primitives" table (two
  rows) and remove the §11 limitation row.
- Updates: `liquers-core/tests/common/minimal_manager.rs` module doc; the source issue.

## Consolidated Findings

- The guide's reason for keeping `notify_removed` private ("raw status writes would bypass the
  status authority") applies to `set_status`, not to a notification. A notification changes no
  state.
- `new_installed` must not hand out a running asset. It returns an `AssetRef` whose status is
  final, with no service loop started. That is the same shape `AssetData::new_ext` +
  field assignment produces in `DefaultAssetManager::set_state` today.
- If the maintainer prefers to keep the surface closed, the work reduces to the guide text, the
  minimal manager's doc, and closing the issue as `closed_not_planned` with that decision.
