# Phase 2: Solution & Architecture — delegating evaluation of a key prefix to a server's asset manager

## Overview

Under prefix authority (Phase 1, question 1, recommended), a working shape exists in three parts.
It is recorded here so the next pass starts from it; it is **not** a plan, because the parts it
rests on are unimplemented and their interfaces still open.

| Part | What | Rides on |
|---|---|---|
| A. Mirror primitive (core) | A public way to create a keyed `AssetRef` whose status, progress, metadata and value are set from outside, with no service loop and no recipe: `AssetRef::new_mirrored(id, key, envref)` plus `apply_remote_info(AssetInfo)`, `apply_remote_progress(..)`, `install_remote_state(State, Metadata)`, `expire_remote(ExpiryReason)` | nothing new; today only `AssetRef::new_installed` exists, which takes a finished state and cannot report progress |
| B. Delegating manager (`liquers-axum`, feature `client`) | `DelegatingAssetManager<E>` wrapping a local manager: `get`/`get_asset`/`set_state`/`set_binary`/`remove`/`expire` for keys under a delegated prefix go to the server; everything else to the inner manager | slice 1's client (`remote-store-and-asset-manager`), the user Assets API, `ws/key`, `ws/events` |
| C. Dependency bridge | A delegated key is a dependency root whose version is the server's: the mirror registers it with `publish_version`; a server `Expired` (from `ws/events`) runs `cascade_expire_dependents` locally; a reconnect audits every mirrored key with `key/version` | `external-index-sync`, `asset-events-over-websocket`, `asset-event-kinds` |

Rejected alternatives:
- **Store-only (slice 1 as is).** Leaves the client evaluating server recipes locally (Phase 1
  problem example).
- **A trusted system API first.** Under prefix authority nothing on the client writes
  manager-owned metadata, so the system API the issue names is not needed for this slice; it
  belongs to replication or split evaluation (Phase 1, question 2).
- **Proxy in `liquers-core`.** Core has no HTTP client and must stay minimal (CLAUDE.md); only the
  mirror primitive is generic.
- **Replacing the whole manager with a remote one.** A client still evaluates local keys and
  queries; a wrapper that routes by prefix keeps `AC-4` trivially true.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `NO-REMOTE-STORE-OR-ASSET-MANAGER` (slice 1) | draft | P2 | the HTTP client, its configuration and placement | yes | yes | `requires` |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | peer identity, AC-7 | yes | for AC-7 | `requires` |
| `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` | draft | P2 | the expiry stream for AC-3 | yes | yes | `requires` |
| `ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS` | draft | P3 | its WebSocket form | yes | yes | `requires`; its priority understates this dependency |
| `ASSET-EVENT-STREAM-CARRIES-ONLY-EXPIRATIONS` | draft | P3 | invalidation on `Updated`/`Removed` (AC-5) | yes | for AC-5 | `requires` |
| `UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS` | draft | P3 | only for delegated queries, a non-goal | no | no | note |

## Interfaces

Sketch only (not a commitment; the blocked questions decide the details):

```rust
// liquers-core/src/assets.rs — part A
impl<E: Environment> AssetRef<E> {
    pub fn new_mirrored(id: u64, key: Key, envref: EnvRef<E>) -> Self;
    pub async fn apply_remote_info(&self, info: &AssetInfo) -> Result<(), Error>;   // status, message, progress
    pub async fn install_remote_state(&self, state: State<E::Value>) -> Result<(), Error>; // value + metadata, Ready
    pub async fn expire_remote(&self, reason: ExpiryReason) -> Result<(), Error>;
}

// liquers-axum/src/client/manager.rs — part B (feature "client")
pub struct DelegationConfig { pub prefix: Key, pub api: ApiClientConfig /* slice 1 */ }
pub struct DelegatingAssetManager<E: Environment, M: AssetManager<E>> { /* inner, delegations, mirrors */ }
impl<E, M> AssetManager<E> for DelegatingAssetManager<E, M> { /* route by prefix */ }
```

`DependencyManagerAccess` and `KeyMutationAccess` forward to the inner manager, so the client has
one dependency graph in which delegated keys are roots.

## Integration Points

- `liquers-core/src/assets.rs`: `AssetRef` constructors beside `new_installed`; `AssetData` must
  accept status/progress writes without a service loop (today only the loop and `run_*` set them).
- The provided `AssetManager` methods `publish_version`, `cascade_expire_dependents` and
  `audit_dependencies` are reused by part C as an external manager would use them.
- `liquers-axum/src/client/` (slice 1) gains `manager.rs` and an events client for `ws/events`.
- Environment building: where a delegation is configured (`EnvironmentBuilder` option or a store
  configuration section) is open; it must not make `liquers-core` depend on HTTP.

## Error Handling

Server errors keep their `ErrorType` (slice 1's mapping of `ErrorDetail`). A lost connection turns
every in-flight mirror into `Error` with a `KeyReadError` naming the server URL (AC-6). A refused
principal is the server's `AccessDenied` error (`access-policy-at-entry-points`), mapped like any other `ErrorType`.

## Relevant Commands

None. No command or registry change; queries over delegated keys use existing commands locally.

## Documentation Architecture

`WEB_API_SPECIFICATION.md` (client section: the routes and messages the proxy depends on, so a
server change that breaks it is visible); `ASSETS.md` (mirrored assets; delegated keys as
externally versioned roots); `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` (the mirror primitive and the
wrapper pattern); the delegation configuration in `ENVIRONMENT_CONFIG.md` or `STORE_CONFIG_FSD.md`.

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | `liquers-core/src/assets.rs` (part A); `liquers-axum/src/client/` (parts B, C); builder or configuration code; tests in both crates |
| Crates and workflows | `liquers-core`, `liquers-axum`; every client that configures a delegation |
| Existing tests | none, if the wrapper routes only delegated prefixes (AC-4) |
| New validation | a two-environment in-process test (server and client) for AC-1..AC-6; the external-manager test suite run against the wrapper |
| Compatibility | Additive; a new `pub` core primitive must not loosen `AssetRef`'s invariants for local assets |
| Data | No new persisted format; a mirror is never stored locally (question 3) |
| Concurrency | Two event sources per mirror (`ws/key` notifications, `ws/events` expiries) can race; the mirror must treat the server's `AssetInfo` re-read as authoritative, as `ws/key` already does |
| Performance | One `ws/key` subscription per live mirror, bounded by the server's `max_subscriptions` (256) per connection; more mirrors need more connections or a multiplexed subscription |
| Security | Credentials in configuration (slice 1's `headers`), replaced by `access-policy-at-entry-points`' sessions |
| Recovery | Remove the delegation from configuration; local behaviour is untouched |
| Certainty | Medium for parts A and C, whose core hooks were inspected at HEAD; low for part B's surface until slice 1 and the identity design land |

## What unblocks Phases 3-4

1. A decision on Phase 1 question 1 (prefix authority recommended) and on questions 2-4.
2. `remote-store-and-asset-manager` slice 1 implemented (client crate placement fixed).
3. `external-index-sync` and `asset-events-over-websocket` implemented (the expiry transport).
4. `access-policy-at-entry-points` decided, at least its principal and session model (AC-7 can be
   deferred to a later slice if the decision is to ship with static headers first).
