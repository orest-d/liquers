# Phase 1: High-Level Design - Expired-Safe Binary Reads

## Feature Name

Expired-safe binary reads (`ASSET-EXPIRED-CACHED-BINARY-READ`)

## Purpose

`AssetData::poll_binary` returns the cached serialized bytes without consulting `Status`, so an
asset that has expired still serves stale bytes through `poll_binary`, `try_poll_binary` and
`get_binary` — while `poll_state` correctly hides them. This brings binary reads under the same
expiration contract as state reads, keeping retained expired bytes reachable only through an
explicit recovery API.

## Verification of the issue at HEAD

The issue was carried forward as "needs verification against PR #11". Confirmed **still live**:
`AssetData::poll_binary` (`liquers-core/src/assets.rs:841`) has no `match self.status`;
`AssetRef::poll_binary`/`try_poll_binary` (`:2450`, `:2456`) delegate to it verbatim; `get_binary`
(`:2387`) polls it before the expiration-aware `get()`. `mark_expired_status` (`:2216`) sets
`Status::Expired` but leaves `lock.binary` populated. PR #11 gated `poll_state` (`:795`) and added
`poll_state_any_status` (`:813`) — it did not touch the binary path.

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
The whole of the change. `AssetData::poll_binary` gains the same status match `poll_state` has;
`AssetRef::poll_binary`/`try_poll_binary`/`get_binary` inherit it; an any-status binary counterpart
is added mirroring `poll_state_any_status`. Manager behaviour is unchanged — `get_asset`/`get(key)`
already treat `Expired` as a cache miss at the request boundary, so the live exposure is an
`AssetRef` held *across* an expiry.

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

1. Should `get_binary` on an expired asset return an error, wait, or recover? `get()` errors when
   expiry is *observed while waiting*, but for an already-expired asset `poll_state` returns `None`
   and `get()` would block. → Phase 2 must define this explicitly; a status check before waiting is
   the likely answer.
2. Should the gate hide bytes for the other non-value statuses too (`Error`, `Cancelled`,
   `Directory`), where `poll_state` returns a metadata-only state? A metadata-only *binary* has no
   natural representation. → Phase 2; narrowing to `Expired` only is the conservative default.
3. What is the recovery API's shape — `poll_binary_any_status` alone, or also
   `get_binary_any_status`? Does `EXPIRATION-RECOVERY-WEB-API` need to grow to cover it?
4. Should the axum handlers surface an expired asset as an error, or re-request from the manager
   (which re-evaluates)? → Affects whether this design touches HTTP behaviour or only fixes a hang.

## References

- `specs/issues/ASSET-EXPIRED-CACHED-BINARY-READ.md` — the issue being addressed
- `specs/design/expiration-safety/` — PR #11, which established the `poll_state` contract
- `specs/reference/ASSETS.md` §"Status and reads", §"Terminal Outcome Contract → Re-evaluation"
- `liquers-core/src/assets.rs:100-116` — the module-level read-contract table, which currently
  documents the bug as intended behaviour ("A cached binary may be returned")
- `specs/issues/EXPIRATION-RECOVERY-WEB-API.md` — related follow-up on recovery APIs
