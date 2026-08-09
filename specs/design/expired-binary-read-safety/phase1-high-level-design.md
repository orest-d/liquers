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
| `AssetRef` | `get_any_status` (`:2433`, alias) | `get_binary_any_status` (**not** an alias — see below) | **missing** |
| `AssetRef` | `try_poll_state` (`:2441`) | `try_poll_binary` (`:2456`) | exists — **gate missing** |
| `AssetManager` | `get_any_status` (`:3281`) | `get_binary_any_status` | **missing** |

Five methods to add, four existing ones to bring under the state contract. The manager-level pair
matters most for recovery: `AssetManager::get_any_status` falls back to loading from the store when
no in-memory asset exists, and that fallback already holds the bytes it deserializes — the binary
counterpart is the cheaper of the two, not an extra cost.

### Statuses with no valid binary

`Error`, `Cancelled` and `Directory` have no binary representation at all. On the state side they
return a *metadata-only* `State`; there is no binary equivalent of that, and `Some(empty bytes)`
would be a lie. The rule is therefore **absence, expressed in whatever the signature allows**:

| Return type | Methods | Result |
|---|---|---|
| `Option<_>` | `poll_binary`, `try_poll_binary`, `poll_binary_any_status` | `None` |
| `Result<_, Error>` | `get_binary` | `Err` |
| `Result<Option<_>, Error>` | `AssetRef::get_binary_any_status`, `AssetManager::get_binary_any_status` | `Ok(None)` |

This is not an exception to the symmetry rule — it *is* the rule applied honestly. The asymmetry
lies in the data (a metadata-only value exists; a metadata-only byte string does not), not in the
interface, and each method still reports it in its own vocabulary.

For the `Result`-returning methods the error is **constructed, not scavenged**: `Error` carries the
asset's own recorded failure (what `State::value_error` yields today), while `Cancelled` and
`Directory` record no error and so need one built for the occasion, via a typed
`liquers_core::error` constructor. Phase 2 picks them.

`Expired` is separate and keeps the state semantics exactly: hidden from normal reads, returned by
the `*_any_status` pair.

### Recovery must not depend on a cached binary

On the state side, `get_any_status` is a plain alias for `poll_state_any_status`, because a retained
value needs no materialising. **The binary side cannot copy that**, and copying it would produce a
recovery API weaker than the thing it mirrors: `AssetData::binary` is populated by two sites and
cleared by roughly ten, so an expired asset very often retains its *value* and no bytes at all —
anything installed via `set_state`/`set_value`, a keyless query asset, a `NonSerializable` persist.
A recovery read that returned `None` there would fail in exactly the case the caller most needs it,
since the escape hatch for a stale-dependency completion is the whole reason the API exists.

So the `get_`/`poll_` distinction, which is vacuous on the state side, is load-bearing on the binary
side: **`get_binary_any_status` serializes on demand from retained expired data; `poll_binary_any_status`
does not.** That is the same relationship `get_binary` and `poll_binary` already have for `Value`,
applied to `Expired` — the symmetry rule taken seriously rather than transcribed. It is also what
closes the originating issue's verification item 4, which asks for consistency "for both cached
binary data and binary data produced by serializing an in-memory value".

Because serialization can fail, `AssetRef::get_binary_any_status` returns
`Result<Option<_>, Error>` rather than `Option<_>` — matching the manager-level signature, and
distinguishing "nothing retained" from "retained but not serializable".

### Expiry is an error

Where a binary read cannot hide expiry behind `None` — that is, in the `Result`-returning methods —
**expiry is an error condition, reported immediately**. `AssetRef::get_binary` on an
already-`Expired` asset returns `Err` rather than waiting; it must not fall through to `get()`,
which would block indefinitely because `poll_state` reports `None` for `Expired` and no further
notification is coming. This matches `get()`'s existing treatment of expiry observed *while
waiting*, and it is what makes the gate on `poll_binary` safe to add: the stale-bytes bug converts
into a prompt error rather than a hang.

**`Expired` is treated uniformly, and that is a decision rather than an oversight.** A second,
unrelated path also produces `Expired`: `finish_run_with_result` relabels a *successfully completed*
asset when its evaluation consumed a stale dependency, marking a fresh result as not-to-be-cached.
That rule exists because a long, expensive calculation can outlive the validity of its own inputs;
restarting risks an unbounded loop, and failing outright can make the result unachievable.

The design nonetheless gives both paths the same reads. Such an asset **is** expired — the only
difference is that it was never `Ready`. Whether a technically-expired result is acceptable is a
judgement only the caller can make, so the caller must make it **explicitly**: by promoting the
asset with `to_override()`, or by reading through the `*_any_status` family. Neither is reachable
by accident, which is the property that matters.

This has a consequence the design owns rather than hides: a caller that today receives bytes for a
stale-dependency completion will receive an error instead. That is recorded as an accepted
regression in Phase 2 §Backward Compatibility and tested, and it is why the recovery API must work
even when no bytes were ever cached (Phase 2 §"Recovery must not depend on a cached binary").

The same answer governs the HTTP layer: `liquers-axum` surfaces an expired asset as an error
response. It does **not** re-request from the manager — re-evaluation is a property of *requesting*
an asset (`get_asset`/`get(key)`), and a handler already holding an `AssetRef` is past that
boundary. Silently recomputing there would hide expiry from the caller, which is the failure mode
this design exists to remove.

Today this territory is accidental rather than defined. `get_binary` on an `Error` asset happens to
end in `Err` only because it falls through `get()` → `serialize_to_binary` → `State::as_bytes`,
which checks `value_error()` first (`state.rs:154`). `Cancelled` stores no error, so `value_error()`
is `None` and the same path serializes a `none` value into *some* byte string. The rule replaces
both accidents with one stated behaviour — and makes an assumption `liquers-axum` already relies on
true by construction: its handler treats a successful `get_binary` under `Status::Error` as
"shouldn't happen" (`handlers.rs:78`).

## Core Interactions

### Query System
No syntax, parsing or planning changes — **but the query language is a consumer.**
`Step::GetAssetBinary` (`interpreter.rs:293-299`), emitted by the plan builder (`plan.rs:1102`) for
a binary resource fetch, calls `AssetRef::get_binary` and is therefore affected by the gate. An
earlier draft of this design asserted there were no consumers outside `liquers-axum`; that was
wrong. See Phase 2 §Integration Points.

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

*All four of the original questions are now closed — the recovery API's shape by the symmetry rule,
the no-valid-binary statuses and their constructed errors by §"Statuses with no valid binary", and
the expiry/HTTP behaviour by §"Expiry is an error". Two consequential questions replace them.*

1. **Does `AssetRef::get()` owe itself the same pre-wait check?** Symmetry plus "expiry is an error"
   says yes: `get()` on an already-`Expired` asset today consults `poll_state`, gets `None`, and
   waits for a notification that will not arrive. That is the identical hang `get_binary` is being
   fixed for, on the state side. → **Recommendation: fix it here.** Leaving `get()` hanging while
   `get_binary` errors would break the very symmetry this design is establishing, and it is a
   handful of lines. If instead it is scoped out, it must be filed as a separate issue rather than
   noted only in review — it is a latent P0-shaped hang in PR #11's work.
2. Does `EXPIRATION-RECOVERY-WEB-API` grow to cover the new manager-level binary recovery read, or
   stay scoped to state? → Affects that issue, not this design's code.

## References

- `specs/issues/ASSET-EXPIRED-CACHED-BINARY-READ.md` — the issue being addressed
- `specs/design/expiration-safety/` — PR #11, which established the `poll_state` contract
- `specs/reference/ASSETS.md` §"Status and reads", §"Terminal Outcome Contract → Re-evaluation"
- `liquers-core/src/assets.rs:100-116` — the module-level read-contract table, which currently
  documents the bug as intended behaviour ("A cached binary may be returned")
- `specs/issues/EXPIRATION-RECOVERY-WEB-API.md` — related follow-up on recovery APIs
