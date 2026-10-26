# Phase 2: Solution & Architecture — A registration asset users can rely on

## Overview

Every way a keyed registration ends was read at HEAD (`liquers-core/src/assets.rs`) and in the
`asset-garbage-collection` design. Only expiry needs a lifecycle change; the rest is either
already harmless to a holder or an explicit call that can refuse. The design therefore splits in
two: a **protection handle** whose meaning is independent of the expiry answer, and an **expiry
rule** that is blocked on question 1.

## How a registration ends today

| Ending | Site at HEAD | Effect on a holder of the asset | With a protection |
|---|---|---|---|
| Expiry | `AssetRef::expire`, expiration monitor (`TimedAsset`), lazy check in `get_nonvolatile_resource_asset` | holder keeps an `Expired` asset; the next request mints a new one (`try_claim_for_run` refuses `Expired`) | **question 1** |
| `remove` | `AssetManager::remove` (decision table in its doc) | holder receives `Removed`; next request mints a new one | refuse (question 2) |
| Replacement | `set_state`, `set_binary`, `to_override` | holder keeps the old asset; the map holds a new one | refuse (question 2) |
| Volatility eviction | `owned_key_asset` on a volatile entry | volatile assets are never owned or registered for reuse | not applicable: a volatile key cannot be protected (refused at protect time) |
| Garbage collection | `collect_garbage` (`asset-garbage-collection`, not implemented) | never collects a held asset (`Arc::strong_count == 1`) | nothing to add: the handle holds an `AssetRef` |
| Environment drop / shutdown | `environment-orderly-shutdown` | everything ends | out of scope |

## Interfaces

The protection handle is independent of question 1.

```rust
// liquers-core/src/assets.rs — sketch; not final until question 1 is answered
/// Held by a user that relies on `key`'s registered asset staying the key's asset.
/// Dropping it releases the protection. Holds the asset strongly, so garbage collection
/// never collects it.
pub struct RegistrationProtection<E: Environment> { asset: AssetRef<E>, key: Key /* private */ }

// AssetManager<E>, provided: an external manager that does not support protection refuses.
async fn protect_key(&self, key: &Key) -> Result<RegistrationProtection<E>, Error> {
    Err(Error::general_error("protection not supported by this asset manager".into()))
}
```

## Integration Points

- **Where the flag lives:** a private counter on the manager's registration record — the
  `Registration<E>` wrapper `asset-garbage-collection` introduces — not on `AssetData`, because
  registration is the manager's (R2). `protect_key` resolves the key like `get` (registering a new
  asset if none), increments the counter under the map entry, and returns the handle; `Drop`
  decrements it. Several holders may protect one key.
- **Refusal sites (question 2):** `remove`, `set_state`, `set_binary`, `to_override` and
  `remove_computed` (`assets-api-admin-operations`) check the counter under the same map operation
  that would replace the entry and return `Error` of type `StatusConflict`, as `remove` already does
  for a directory.
- **R4 is unaffected:** protection only prevents an ending; it never re-registers an asset.
- **wasm32 / inline manager:** the same counter on its `Mutex<HashMap<_, Registration<E>>>` value.

## Error Handling

A refused explicit replacement returns an `Error` of type `StatusConflict` (typed constructor, as
`remove` uses for a directory) naming the key and that it is protected. `protect_key` on a volatile
key, a directory or a query fails the same way. An external manager that does not override
`protect_key` returns a general error; no caller is forced to handle protection.

## Relevant Commands

None. No command or `specs/command_registry.yaml` change; an HTTP route is a non-goal (see Risks).

## Documentation Architecture

Once question 1 is answered: `specs/reference/ASSETS.md` §Key ownership (protection as the one
exception to R2), `specs/reference/ASSET_LIFECYCLE.md` (only for recompute in place: the new
transition), `specs/guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §7. No `specs/README.md` change.

## Why expiry blocks Phases 3-4

- *Downgrade* needs no new code beyond the handle, but AC-5 then says users diverge on expiry, the
  one event a channel must survive. A consumer accepting that would be served equally well by
  holding the `AssetRef` and re-getting on `Expired` (alternative C).
- *Refuse* is a small change (skip `expire` while the counter is non-zero, defer it to release) but
  serves stale values to every dependent; it contradicts `ASSETS.md` §Expiration.
- *Recompute in place* needs `Expired` → `Recipe` in `AssetData::set_status`, a claim arm for
  `Expired` in `try_claim_for_run` / `try_claim_for_run_inline`, a new notification so holders can
  tell "re-running" from "expired for good", and an audit of every reader that treats `Expired` as
  terminal (`wait_for_dependency`'s `stale_dependency` rule, `finalize_status_with_version`, the
  dependency manager's cascade gate). Its size depends on that audit, which is only worth doing for
  a consumer that needs it.

## Alternatives

- **A. Protection flag with downgrade (issue option 1).** Specified above; blocked only in that it
  may not serve any consumer.
- **B. Recompute in place (option 3).** The only answer under which users converge across expiry;
  `XL`-adjacent lifecycle change.
- **C. Follow the key, not the asset (recommended direction).** Users converge on the *key*: a
  subscription that yields the key's current asset and re-resolves it on `Expired`, `Removed` or
  replacement. It needs no lifecycle change and builds on a key-scoped event stream
  (`external-index-sync`, `ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS`). It does not give "one
  asset forever", which may be exactly what a channel consumer does not need.
- **D. Do nothing until a consumer exists.** What `asset-registration-contract` recommends.

## Known-Issue Preflight

| Issue | Status | Impact | Blocking? |
|---|---|---|---|
| `ASSET-REGISTRATION-OWNERSHIP-CONTRACT` | draft, designed (needs-decision) | defines R2/R4 that a protection modifies | yes, as ordering (`requires`) |
| `CORE-ASSET-GC` | accepted, designed | introduces the `Registration<E>` record the counter lives on | no; the counter can live on a plain map wrapper if GC lands later |
| `TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE` | draft | `to_override` and expiry interact; refusal (AC-3) avoids it for protected keys | no |

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (both managers' maps, `remove`, `set_state`, `set_binary`, `to_override`, expiry path); `tests/common/manager_scenarios.rs` |
| Crates and workflows affected | `liquers-core`; every evaluation path reads the maps (unchanged while no protection is held) |
| Existing tests likely to change | none for alternative A/C; option B changes tests asserting an `Expired` asset is never re-run |
| New validation | not specified (Phase 3 blocked); would be a parametric manager scenario over queued, inline and external managers |
| Compatibility / data / concurrency / performance / security | no serialized change; a forgotten handle pins memory and blocks `remove` (a denial-of-service vector if exposed over HTTP — a route would need authorisation) |
| Recovery | revert; the handle is additive |
| Certainty and open questions | high on the inventory; the expiry rule (question 1) is unknown by nature |
