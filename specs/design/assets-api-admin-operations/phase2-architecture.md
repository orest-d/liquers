# Phase 2: Solution & Architecture — Manager status and a guarded removal

## Overview

Move the body of `AssetManager::remove` (`liquers-core/src/assets.rs`) into a private generic
helper that takes a guard; `remove` calls it with `RemoveGuard::Any`, a new provided method
`remove_computed` with `RemoveGuard::ComputedOnly`, which turns the `Delete` decision into a
`StatusConflict` before anything is touched. In `liquers-axum`, `POST key/remove_computed/{*key}`
(mutation switches as `DELETE key/data`) calls it, and `GET manager` reports `eval_mode()` and
`is_started()`.

Rejected: a check in the handler followed by `remove` (races, AC-3); taking `key_mutation_lock`
in the handler (the lock is not reentrant, `remove` takes it); a guard argument on `remove`
(changes a provided method every implementor and binding sees); `Status::Source` in `info` as the
client's only defence (the problem itself).

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `CORE-SESSION-AND-KEY-ACL` (`access-policy-at-entry-points`) | accepted | P2 | Will classify the new routes (`Write`, admin read) | No | No | None; routes follow the existing switches |
| `ENVIRONMENT-MANAGER-REFERENCE-CYCLE` | draft | P2 | Touches the manager, not `remove` | No | No | None |
| `ASSET-CANCEL-DURING-PROCESSING-FINISHES-READY` | closed | — | `remove` cancels an in-flight asset first (`cancel_for_replacement`); reused as is | — | No | None |

## Interfaces

`liquers-core/src/assets.rs`:

```rust
/// Which decisions of the removal table may proceed. Private.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RemoveGuard { Any, ComputedOnly }

/// The body of `AssetManager::remove`, unchanged, plus: under `ComputedOnly`, a `Delete`
/// decision returns `Error::status_conflict(key, decisive_or_source, "remove_computed")`
/// before any cancel, unmap or store write. Takes `key_mutation_lock`.
async fn remove_keyed<E: Environment, M: AssetManager<E> + ?Sized>(
    manager: &M, key: &Key, guard: RemoveGuard) -> Result<(), Error>;

// AssetManager<E>, provided:
async fn remove(&self, key: &Key) -> Result<(), Error> { remove_keyed(self, key, RemoveGuard::Any).await }
/// As `remove`, but only a computed value is removed; a `Source`, an `Override`, or any value of a
/// key without a recipe is a `StatusConflict`. Never triggers an evaluation.
async fn remove_computed(&self, key: &Key) -> Result<(), Error> {
    remove_keyed(self, key, RemoveGuard::ComputedOnly).await
}
```

Async, as `remove` (store I/O). `?Sized` and the trait's `MaybeSend + MaybeSync` supertraits keep
the future `Send` on native and `?Send` on wasm, as the existing provided methods are.

`liquers-axum/src/assets/common.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ManagerInfo { pub eval_mode: &'static str, pub is_started: bool }  // "Queued" | "Inline"
```

`liquers-axum/src/assets/key_handlers.rs`: `pub async fn key_remove_computed_handler<E>(State,
Path) -> Response` (answers `RemoveResult` like `key_remove_handler`; a `StatusConflict` for a
directory gets the `key/removedir` hint, one for a user value the hint "use DELETE key/data to
remove a user value"); `pub async fn manager_info_handler<E>(State) -> Response`.

## Integration Points

- `liquers-core/src/assets.rs`: `AssetManager::remove` (body moved), new `remove_computed`, the
  private `RemoveGuard` / `remove_keyed`. `DefaultAssetManager` and `ImmediateAssetManager` inherit
  both. The trait-object-free generic helper sits next to `dropped_computed_metadata`.
- `liquers-axum/src/assets/builder.rs` `build`: `self.optional(router, "{b}/key/remove_computed/
  {*key}", mutations, post(kh::key_remove_computed_handler), get(...))`; `router.route("{b}/manager",
  get(kh::manager_info_handler))` unconditionally. Module doc's route-presence rule lists both.
- Callers of `remove` elsewhere (`removedir`, `liquers-lib` `ns-mem`, `liquers-py`, `liquers-web`)
  are unchanged: same signature, same behaviour.

## Error Handling

| Case | Error | HTTP |
|---|---|---|
| user value or no recipe | `Error::status_conflict(key, status, "remove_computed")` | 409 |
| directory | `Error::status_conflict(key, Status::Directory, "remove_computed")` | 409 |
| unknown key | `Error::key_not_found(key)` | 404 |
| store failure | the store's error, as `remove` | per §3.1 |

For a key with no recipe and nothing live, the decisive status is the stored one; when the record
has none, `Status::Source` is reported (a stored value without a recipe is user data).

## Relevant Commands

None. Namespaces: none. (`ns-mem`'s removal keeps calling `remove`.)

## Documentation Architecture

- `specs/reference/WEB_API_SPECIFICATION.md` (reference, axum): §5.4 table row for `POST
  key/remove_computed`, a sentence in "Removal" on the guard; a `GET manager` row (new §5.5
  paragraph "Manager status"); §5.6 switch table lists the GET alternative and the `read_only()`
  omission; History row, `reviewed:`.
- `specs/reference/ASSETS.md` (reference, core/assets): `Removed` is also sent by
  `remove_computed`; History row, `reviewed:`.
- `specs/guides/WEB_API_GUIDE.md` (guide): §7 one curl example of the refusal; History row.
- Proposed `affects_docs`: the three above.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `assets.rs` (remove body), `builder.rs`, `key_handlers.rs`, `common.rs`, one core and one axum test file |
| Crates and workflows affected | core (asset manager), axum (Assets API) |
| Existing tests likely to change | none; `amr01`-`amr53` and `aae*` removal tests must pass unchanged (they prove the move kept `remove`) |
| New validation | `amr70`-`amr74` in core, `aae93`-`aae96` in axum (Phase 3) |
| Compatibility | additive: a provided trait method, two routes; no serialized format changes |
| Concurrency | the guard is evaluated inside `key_mutation_lock`, after the decisive status is read, so a concurrent `set_binary` is either before (refused) or after (survives) |
| Performance / security | none; the new mutation obeys `read_only()` |
| Recovery | revert the axum commit to hide the routes; the core method is unused without it |
| Certainty | high: every symbol opened; the decision table is `remove`'s |
