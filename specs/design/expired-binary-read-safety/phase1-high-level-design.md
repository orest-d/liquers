# Phase 1: High-Level Design - Expired-Safe Binary Reads

## Feature Name

Expired-safe binary reads (`ASSET-EXPIRED-CACHED-BINARY-READ`)

## Purpose

`AssetData::poll_binary` returns the cached serialized bytes without consulting `Status`, so an
asset that has expired still serves stale bytes through `poll_binary`, `try_poll_binary` and
`get_binary` — while `poll_state` correctly hides them. This brings binary reads under the same
expiration contract as state reads, keeping retained expired bytes reachable only through an
explicit recovery API.

The fix is stated as a **symmetry rule, not a patch**: every value-read method has a `*_binary`
counterpart with analogous behaviour. The bug is one missing status check; the rule is what keeps
the next one from appearing, since a binary method that has no state twin has no contract to be
checked against.

## Verification of the issue at HEAD

The issue was carried forward as "needs verification against PR #11". Confirmed **still live**:
`AssetData::poll_binary` (`liquers-core/src/assets.rs:841`) has no `match self.status`;
`AssetRef::poll_binary`/`try_poll_binary` (`:2450`, `:2456`) delegate to it verbatim; `get_binary`
(`:2387`) polls it before the expiration-aware `get()`. `mark_expired_status` (`:2216`) sets
`Status::Expired` but leaves `lock.binary` populated. PR #11 gated `poll_state` (`:795`) and added
`poll_state_any_status` (`:813`) — it did not touch the binary path.

## Read-API symmetry (governing principle)

Every method that reads a value gets a `*_binary` counterpart behaving analogously — same waiting
semantics, same status gating, same lock behaviour, differing only in returning
`(Arc<Vec<u8>>, Arc<Metadata>)` instead of `State<E::Value>`. Auditing the three layers against
this rule gives the exact scope; `AssetManager::get`/`get_asset` are excluded because they return
an `AssetRef`, not a value.

| Layer | State read | Binary counterpart | Status |
|---|---|---|---|
| `AssetData` | `poll_state` (`:769`) | `poll_binary` (`:841`) | exists — **gate missing** |
| `AssetData` | `poll_state_any_status` (`:813`) | `poll_binary_any_status` | **missing** |
| `AssetRef` | `get` (`:2325`) | `get_binary` (`:2387`) | exists — **gate missing** |
| `AssetRef` | `poll_state` (`:2419`) | `poll_binary` (`:2450`) | exists — **gate missing** |
| `AssetRef` | `poll_state_any_status` (`:2425`) | `poll_binary_any_status` | **missing** |
| `AssetRef` | `get_any_status` (`:2433`, alias) | `get_binary_any_status` (alias) | **missing** |
| `AssetRef` | `try_poll_state` (`:2441`) | `try_poll_binary` (`:2456`) | exists — **gate missing** |
| `AssetManager` | `get_any_status` (`:3281`) | `get_binary_any_status` | **missing** |

Five methods to add, four existing ones to bring under the state contract. The manager-level pair
matters most for recovery: `AssetManager::get_any_status` falls back to loading from the store when
no in-memory asset exists, and that fallback already holds the bytes it deserializes — the binary
counterpart is the cheaper of the two, not an extra cost.

## Core Interactions

### Query System
No interaction. No query syntax, parsing or planning changes.

### Store System
`AssetRef::save_to_store` (`:1944`) obtains bytes via `poll_binary()` and falls back to
`serialize_to_binary()`. Gating `poll_binary` on status must not break persistence, so the
internal write path needs a status-blind accessor. `serialize_to_binary` (`:2012`) is already safe:
it goes through `poll_state()`, which returns `None` for `Expired`.

### Command System
No new commands, no namespace changes.

### Asset System
The whole of the change — see the symmetry table above. `AssetData::poll_binary` gains the status
match `poll_state` has; the `AssetRef` wrappers inherit it; five any-status counterparts are added
across `AssetData`, `AssetRef` and `AssetManager`. Manager *routing* is unchanged —
`get_asset`/`get(key)` already treat `Expired` as a cache miss at the request boundary, so the live
exposure is an `AssetRef` held *across* an expiry.

### Value Types
None.

### Web/API
`liquers-axum` query handlers (`src/query/handlers.rs:61`, `:175`) poll `poll_binary` in a loop and
return the first bytes they see — the concrete instance of the bug. Their `match` on status has a
`_ =>` arm that treats `Expired` as "still processing", so once `poll_binary` is gated they will
spin until the 30 s timeout unless the arm is made explicit.

### UI
None.

## Crate Placement

**liquers-core** (`src/assets.rs`) — the contract and its enforcement live where `Status` and
`poll_state` live. **liquers-axum** — audit and fix the two polling loops that consume the changed
contract. No other crate reads these methods (`liquers-py`'s `get_binary` is the unrelated `Cache`
trait).

## Open Questions

*Question 3 of the original draft — the recovery API's shape — is closed by the symmetry rule: the
full `*_any_status` set is in scope at all three layers.*

1. Where "analogous" is underdetermined, `get_binary` is the hard case. `get()` errors only when
   expiry is observed *while waiting*; for an already-expired asset it consults `poll_state`, gets
   `None`, and waits. `get_binary`'s current short-circuit hides that. Analogous behaviour must be
   defined as a status check *before* waiting, and Phase 2 must say whether the same check is owed
   to `get()` itself — if it is, that is a second (smaller) bug in PR #11's work, not part of this
   one.
2. Should the gate hide bytes for the other non-value statuses too (`Error`, `Cancelled`,
   `Directory`), where `poll_state` returns a *metadata-only* state? Strict symmetry says yes, but
   a metadata-only binary has no natural representation — `None` and `Some(empty bytes)` both
   misrepresent it. → Phase 2; this is where the symmetry rule needs its one documented exception.
3. Does `EXPIRATION-RECOVERY-WEB-API` grow to cover the new manager-level binary recovery read, or
   stay scoped to state? → Affects that issue, not this design's code.
4. Should the axum handlers surface an expired asset as an error, or re-request from the manager
   (which re-evaluates)? → Affects whether this design touches HTTP behaviour or only fixes a hang.

## References

- `specs/issues/ASSET-EXPIRED-CACHED-BINARY-READ.md` — the issue being addressed
- `specs/design/expiration-safety/` — PR #11, which established the `poll_state` contract
- `specs/reference/ASSETS.md` §"Status and reads", §"Terminal Outcome Contract → Re-evaluation"
- `liquers-core/src/assets.rs:100-116` — the module-level read-contract table, which currently
  documents the bug as intended behaviour ("A cached binary may be returned")
- `specs/issues/EXPIRATION-RECOVERY-WEB-API.md` — related follow-up on recovery APIs
